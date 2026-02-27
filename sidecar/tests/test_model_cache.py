"""Tests for model cache management."""

from __future__ import annotations

import errno
import hashlib
import json
import os
import shutil
import tempfile
import threading
import time
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest

import openvoicy_sidecar.model_cache as model_cache
from openvoicy_sidecar.model_cache import (
    DOWNLOAD_CHUNK_SIZE,
    CacheCorruptError,
    CacheLock,
    DiskFullError,
    DownloadProgress,
    LockError,
    ModelCacheError,
    ModelCacheManager,
    ModelFileInfo,
    ModelInUseError,
    ModelManifest,
    ModelStatus,
    NetworkError,
    check_disk_space,
    compute_sha256,
    download_file,
    build_download_headers,
    download_with_mirrors,
    format_bytes,
    get_cache_directory,
    handle_model_install,
    handle_model_purge_cache,
    verify_file,
    verify_sha256,
    verify_manifest,
)
from openvoicy_sidecar.protocol import Request


# === Fixtures ===


@pytest.fixture
def temp_cache_dir(tmp_path):
    """Create a temporary cache directory."""
    cache_dir = tmp_path / "cache" / "models"
    cache_dir.mkdir(parents=True)

    # Patch get_cache_directory to return our temp dir
    with patch("openvoicy_sidecar.model_cache.get_cache_directory", return_value=cache_dir):
        yield cache_dir


@pytest.fixture
def sample_manifest():
    """Create a sample model manifest."""
    return ModelManifest(
        model_id="test-model",
        revision="abc123",
        display_name="Test Model",
        total_size_bytes=1000,
        files=[
            ModelFileInfo(
                path="model.bin",
                size_bytes=800,
                sha256="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",  # sha256 of empty file
                primary_url="http://example.com/model.bin",
                mirror_urls=["http://mirror.example.com/model.bin"],
            ),
            ModelFileInfo(
                path="config.json",
                size_bytes=200,
                sha256="",
                primary_url="http://example.com/config.json",
            ),
        ],
    )


@pytest.fixture
def mock_http_server(tmp_path):
    """Create files that can be served via mocked HTTP."""
    server_root = tmp_path / "server"
    server_root.mkdir()

    # Create test files
    model_file = server_root / "model.bin"
    model_file.write_bytes(b"x" * 800)

    config_file = server_root / "config.json"
    config_file.write_text('{"version": "1.0"}')

    return server_root


# === Unit Tests: Utilities ===


class TestUtilities:
    """Tests for utility functions."""

    def test_format_bytes(self):
        """Should format bytes for human readability."""
        assert format_bytes(0) == "0.0 B"
        assert format_bytes(1024) == "1.0 KB"
        assert format_bytes(1024 * 1024) == "1.0 MB"
        assert format_bytes(1024 * 1024 * 1024) == "1.0 GB"

    def test_compute_sha256(self, tmp_path):
        """Should compute correct SHA-256 hash."""
        test_file = tmp_path / "test.txt"
        test_file.write_text("hello world")

        sha256 = compute_sha256(test_file)
        # Known hash of "hello world"
        assert sha256 == "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"

    def test_compute_sha256_empty_file(self, tmp_path):
        """Should handle empty file."""
        test_file = tmp_path / "empty.txt"
        test_file.write_bytes(b"")

        sha256 = compute_sha256(test_file)
        assert sha256 == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"


# === Unit Tests: Disk Space ===


class TestDiskSpace:
    """Tests for disk space checking."""

    def test_check_disk_space_sufficient(self, temp_cache_dir):
        """Should pass when sufficient space available."""
        # Request a small amount that should always be available
        check_disk_space(1024)  # 1KB

    def test_check_disk_space_insufficient(self, temp_cache_dir):
        """Should raise DiskFullError when insufficient space."""
        # Request an unreasonably large amount
        with pytest.raises(DiskFullError) as exc_info:
            check_disk_space(10**18)  # 1 exabyte

        assert exc_info.value.required > 0
        assert exc_info.value.available > 0


# === Unit Tests: Cache Lock ===


class TestCacheLock:
    """Tests for cache locking."""

    def test_lock_acquire_release(self, temp_cache_dir):
        """Should acquire and release lock."""
        lock = CacheLock(timeout=5)

        assert lock.acquire()
        lock.release()

    def test_lock_context_manager(self, temp_cache_dir):
        """Should work as context manager."""
        with CacheLock(timeout=5) as lock:
            # Inside context, lock is held
            pass
        # Outside context, lock is released

    def test_lock_timeout(self, temp_cache_dir):
        """Should timeout if lock held by another."""
        lock1 = CacheLock(timeout=0.5)
        lock2 = CacheLock(timeout=0.5)

        assert lock1.acquire()

        # lock2 should timeout
        start = time.time()
        assert not lock2.acquire()
        elapsed = time.time() - start
        assert elapsed >= 0.4

        lock1.release()

    def test_lock_reentrant_fails(self, temp_cache_dir):
        """Same process can't acquire lock twice (different instance)."""
        lock1 = CacheLock(timeout=0.5)
        lock2 = CacheLock(timeout=0.5)

        with lock1:
            # Second lock should timeout
            assert not lock2.acquire()


# === Unit Tests: Verification ===


class TestVerification:
    """Tests for file verification."""

    def test_verify_file_exists_and_valid(self, tmp_path):
        """Should return True for valid file."""
        test_file = tmp_path / "test.bin"
        test_file.write_bytes(b"test content")

        # With correct size, no hash check
        assert verify_file(test_file, "", len("test content"))

    def test_verify_file_not_exists(self, tmp_path):
        """Should return False for missing file."""
        assert not verify_file(tmp_path / "nonexistent.bin", "", 100)

    def test_verify_file_wrong_size(self, tmp_path):
        """Should return False for wrong size."""
        test_file = tmp_path / "test.bin"
        test_file.write_bytes(b"test")

        assert not verify_file(test_file, "", 1000)  # Wrong size

    def test_verify_file_wrong_hash(self, tmp_path):
        """Should return False for wrong hash."""
        test_file = tmp_path / "test.bin"
        test_file.write_bytes(b"test content")

        assert not verify_file(test_file, "deadbeef", len("test content"))

    def test_verify_file_skip_placeholder_hash(self, tmp_path):
        """Should skip hash check for placeholder value."""
        test_file = tmp_path / "test.bin"
        test_file.write_bytes(b"test content")

        assert verify_file(test_file, "VERIFY_ON_FIRST_DOWNLOAD", len("test content"))

    def test_verify_sha256_matches(self, tmp_path):
        """Should return actual hash and success for a matching SHA-256."""
        test_file = tmp_path / "test.bin"
        test_file.write_bytes(b"test content")
        expected = compute_sha256(test_file)

        ok, actual = verify_sha256(test_file, expected)
        assert ok is True
        assert actual == expected

    def test_verify_sha256_mismatch(self, tmp_path):
        """Should return actual hash when SHA-256 does not match."""
        test_file = tmp_path / "test.bin"
        test_file.write_bytes(b"test content")

        ok, actual = verify_sha256(test_file, "0" * 64)
        assert ok is False
        assert actual == compute_sha256(test_file)


class TestAtomicActivation:
    """Tests for atomic activation of staged model directories."""

    def test_activate_staged_model_dir_moves_into_final_path(self, tmp_path):
        """Should rename staged model directory into final location."""
        staged_dir = tmp_path / ".partial" / "test-model"
        final_dir = tmp_path / "test-model"
        staged_dir.mkdir(parents=True)
        (staged_dir / "model.bin").write_bytes(b"new-model")

        model_cache._activate_staged_model_dir(staged_dir, final_dir)

        assert final_dir.exists()
        assert (final_dir / "model.bin").read_bytes() == b"new-model"
        assert not staged_dir.exists()

    def test_activate_staged_model_dir_restores_previous_on_rename_error(self, tmp_path):
        """Should restore previous final directory if activation rename fails."""
        staged_dir = tmp_path / ".partial" / "test-model"
        final_dir = tmp_path / "test-model"
        staged_dir.mkdir(parents=True)
        final_dir.mkdir(parents=True)
        (staged_dir / "model.bin").write_bytes(b"new-model")
        (final_dir / "model.bin").write_bytes(b"old-model")

        original_rename = os.rename

        def fail_on_staged_to_final(src, dst):
            src_path = Path(src)
            dst_path = Path(dst)
            if src_path == staged_dir and dst_path == final_dir:
                raise OSError("simulated rename failure")
            return original_rename(src, dst)

        with patch("openvoicy_sidecar.model_cache.os.rename", side_effect=fail_on_staged_to_final):
            with pytest.raises(OSError, match="simulated rename failure"):
                model_cache._activate_staged_model_dir(staged_dir, final_dir)

        assert final_dir.exists()
        assert (final_dir / "model.bin").read_bytes() == b"old-model"
        assert staged_dir.exists()


# === Unit Tests: ModelManifest ===


class TestModelManifest:
    """Tests for ModelManifest parsing."""

    def test_from_dict(self):
        """Should parse manifest from dictionary."""
        data = {
            "model_id": "test-model",
            "revision": "v1",
            "display_name": "Test Model",
            "total_size_bytes": 1000,
            "files": [
                {
                    "path": "model.bin",
                    "size_bytes": 800,
                    "sha256": "abc123",
                }
            ],
            "mirrors": [
                {"url": "http://example.com/model.bin"}
            ],
        }

        manifest = ModelManifest.from_dict(data)

        assert manifest.model_id == "test-model"
        assert manifest.revision == "v1"
        assert len(manifest.files) == 1
        assert manifest.files[0].path == "model.bin"


# === Unit Tests: Download ===


class TestDownload:
    """Tests for download functions."""

    def test_download_file_mock(self, tmp_path):
        """Should download file successfully."""
        dest = tmp_path / "downloaded.bin"
        content = b"test file content"

        # Mock urllib
        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 200
            mock_response.headers = {"Content-Length": str(len(content))}
            mock_response.read.side_effect = [content, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)

            mock_urlopen.return_value = mock_response

            download_file("http://example.com/file.bin", dest, len(content))

        assert dest.exists()
        assert dest.read_bytes() == content

    def test_download_file_aborts_on_content_length_mismatch(self, tmp_path):
        """Should fail early when server Content-Length disagrees with manifest size."""
        dest = tmp_path / "downloaded.bin"
        content = b"test file content"

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 200
            mock_response.headers = {"Content-Length": str(len(content) + 5)}
            mock_response.read.side_effect = [content, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            with pytest.raises(NetworkError) as exc_info:
                download_file("http://example.com/file.bin", dest, len(content))

        assert "content-length mismatch" in exc_info.value.message.lower()
        assert not dest.exists()

    def test_download_file_aborts_on_resumed_content_length_mismatch(self, tmp_path):
        """Should fail early when resumed response length is inconsistent."""
        dest = tmp_path / "downloaded.bin"
        dest.write_bytes(b"x" * 10)  # existing partial

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 206
            # Expected remaining is 10 bytes (20 total - 10 existing).
            mock_response.headers = {
                "Content-Length": "12",
                "Content-Range": "bytes 10-19/20",
            }
            mock_response.read.side_effect = [b"y" * 12, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            with pytest.raises(NetworkError) as exc_info:
                download_file("http://example.com/file.bin", dest, expected_size=20)

            request = mock_urlopen.call_args.args[0]
            assert request.headers.get("Range") == "bytes=10-"

        assert "content-length mismatch" in exc_info.value.message.lower()
        assert dest.read_bytes() == b"x" * 10

    def test_download_file_resumes_when_content_range_matches_partial(self, tmp_path):
        """Should append bytes when server honors Range request with matching Content-Range."""
        dest = tmp_path / "downloaded.bin"
        dest.write_bytes(b"x" * 10)

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 206
            mock_response.headers = {
                "Content-Length": "10",
                "Content-Range": "bytes 10-19/20",
            }
            mock_response.read.side_effect = [b"y" * 10, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            download_file("http://example.com/file.bin", dest, expected_size=20)

            request = mock_urlopen.call_args.args[0]
            assert request.headers.get("Range") == "bytes=10-"

        assert dest.read_bytes() == (b"x" * 10) + (b"y" * 10)

    def test_download_file_restarts_when_server_ignores_range(self, tmp_path):
        """Should restart from zero when Range is ignored and server returns 200."""
        dest = tmp_path / "downloaded.bin"
        dest.write_bytes(b"stale-partial")
        fresh = b"01234567890123456789"

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 200
            mock_response.headers = {"Content-Length": str(len(fresh))}
            mock_response.read.side_effect = [fresh, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            download_file("http://example.com/file.bin", dest, expected_size=len(fresh))

            request = mock_urlopen.call_args.args[0]
            assert request.headers.get("Range") == "bytes=13-"

        assert dest.read_bytes() == fresh

    def test_download_file_aborts_on_invalid_content_range_start(self, tmp_path):
        """Should fail when resumed response starts at the wrong byte offset."""
        dest = tmp_path / "downloaded.bin"
        dest.write_bytes(b"x" * 10)

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 206
            mock_response.headers = {
                "Content-Length": "10",
                "Content-Range": "bytes 0-9/20",
            }
            mock_response.read.side_effect = [b"y" * 10, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            with pytest.raises(NetworkError) as exc_info:
                download_file("http://example.com/file.bin", dest, expected_size=20)

        assert "content-range start mismatch" in exc_info.value.message.lower()
        assert dest.read_bytes() == b"x" * 10

    def test_download_file_checks_final_size_after_download(self, tmp_path):
        """Should fail when final written file size differs from expected."""
        dest = tmp_path / "downloaded.bin"
        content = b"short"

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 200
            mock_response.headers = {}  # Force post-download size check path.
            mock_response.read.side_effect = [content, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            with pytest.raises(NetworkError) as exc_info:
                download_file("http://example.com/file.bin", dest, expected_size=10)

        assert "size mismatch" in exc_info.value.message.lower()
        assert dest.exists()
        assert dest.stat().st_size == len(content)

    def test_download_file_maps_enospc_to_disk_full(self, tmp_path):
        """Should surface ENOSPC as structured DiskFullError with recovery guidance."""
        dest = tmp_path / "downloaded.bin"
        content = b"test file content"

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_response = MagicMock()
            mock_response.status = 200
            mock_response.headers = {"Content-Length": str(len(content))}
            mock_response.read.side_effect = [content, b""]
            mock_response.__enter__ = lambda s: s
            mock_response.__exit__ = MagicMock(return_value=False)
            mock_urlopen.return_value = mock_response

            with (
                patch(
                    "builtins.open",
                    side_effect=OSError(errno.ENOSPC, "No space left on device"),
                ),
                patch(
                    "openvoicy_sidecar.model_cache.get_cache_directory",
                    return_value=tmp_path,
                ),
                patch(
                    "openvoicy_sidecar.model_cache.shutil.disk_usage",
                    return_value=(1024, 924, 100),
                ),
            ):
                with pytest.raises(DiskFullError) as exc_info:
                    download_file(
                        "http://example.com/file.bin",
                        dest,
                        expected_size=len(content),
                    )

        error = exc_info.value
        assert error.required == len(content)
        assert error.available == 100
        assert "purge_model_cache" in error.message

    def test_build_download_headers_uses_hf_token_for_trusted_hf_urls_only(self):
        """Should include Authorization header only for trusted Hugging Face HTTPS URLs."""
        with patch.dict(os.environ, {"HF_TOKEN": "hf_test_token"}, clear=False):
            headers = build_download_headers(
                existing_size=128,
                url="https://huggingface.co/nvidia/model/resolve/main/model.bin",
            )

        assert headers["Range"] == "bytes=128-"
        assert headers["Authorization"] == "Bearer hf_test_token"

    def test_build_download_headers_omits_hf_token_for_non_hf_urls(self):
        """Should never attach Authorization header for non-HF hosts."""
        with patch.dict(os.environ, {"HF_TOKEN": "hf_test_token"}, clear=False):
            headers = build_download_headers(
                existing_size=64,
                url="https://example.com/model.bin",
            )

        assert headers["Range"] == "bytes=64-"
        assert "Authorization" not in headers

    def test_download_file_adds_authorization_header_for_trusted_hf_url(self, tmp_path):
        """Should attach HF auth header to trusted Hugging Face download requests."""
        dest = tmp_path / "downloaded.bin"
        content = b"test file content"

        with patch.dict(os.environ, {"HF_TOKEN": "hf_header_token"}, clear=False):
            with patch("urllib.request.urlopen") as mock_urlopen:
                mock_response = MagicMock()
                mock_response.status = 200
                mock_response.headers = {"Content-Length": str(len(content))}
                mock_response.read.side_effect = [content, b""]
                mock_response.__enter__ = lambda s: s
                mock_response.__exit__ = MagicMock(return_value=False)
                mock_urlopen.return_value = mock_response

                download_file(
                    "https://huggingface.co/nvidia/model/resolve/main/file.bin",
                    dest,
                    len(content),
                )

                request = mock_urlopen.call_args.args[0]
                auth_header = request.headers.get("Authorization")
                if auth_header is None:
                    auth_header = request.headers.get("authorization")
                assert auth_header == "Bearer hf_header_token"

    def test_download_file_does_not_add_authorization_header_for_non_hf_url(self, tmp_path):
        """Should not attach HF auth header to non-HF mirrors."""
        dest = tmp_path / "downloaded.bin"
        content = b"test file content"

        with patch.dict(os.environ, {"HF_TOKEN": "hf_header_token"}, clear=False):
            with patch("urllib.request.urlopen") as mock_urlopen:
                mock_response = MagicMock()
                mock_response.status = 200
                mock_response.headers = {"Content-Length": str(len(content))}
                mock_response.read.side_effect = [content, b""]
                mock_response.__enter__ = lambda s: s
                mock_response.__exit__ = MagicMock(return_value=False)
                mock_urlopen.return_value = mock_response

                download_file("https://example.com/file.bin", dest, len(content))

                request = mock_urlopen.call_args.args[0]
                auth_header = request.headers.get("Authorization")
                if auth_header is None:
                    auth_header = request.headers.get("authorization")
                assert auth_header is None

    def test_download_with_mirrors_fallback(self, tmp_path):
        """Should try mirrors on primary failure."""
        dest = tmp_path / "downloaded.bin"
        content = b"test content"

        call_count = [0]

        def mock_download(url, path, size, callback):
            call_count[0] += 1
            if "primary" in url:
                raise NetworkError("Primary failed", url)
            # Mirror succeeds
            path.write_bytes(content)

        file_info = ModelFileInfo(
            path="file.bin",
            size_bytes=len(content),
            sha256="",
            primary_url="http://primary.example.com/file.bin",
            mirror_urls=["http://mirror.example.com/file.bin"],
        )

        with patch("openvoicy_sidecar.model_cache.download_file", side_effect=mock_download):
            with patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_MAX_ATTEMPTS", 1):
                download_with_mirrors(file_info, dest)

        assert call_count[0] == 2  # Primary + mirror
        assert dest.read_bytes() == content

    def test_download_with_mirrors_falls_back_on_hash_mismatch(self, tmp_path):
        """Should try next mirror when post-download verification fails."""
        dest = tmp_path / "downloaded.bin"
        calls: list[str] = []

        primary_url = "http://primary.example.com/file.bin"
        mirror_url = "http://mirror.example.com/file.bin"

        def mock_download(url, path, size, callback):
            calls.append(url)
            if url == primary_url:
                path.write_bytes(b"bad")
            else:
                path.write_bytes(b"good")

        def verify_download(url: str, path: Path) -> None:
            if url == primary_url:
                raise CacheCorruptError(
                    "Downloaded model file hash mismatch. File may be corrupted or incomplete.",
                    str(path),
                    details={"recoverable": True},
                )

        file_info = ModelFileInfo(
            path="file.bin",
            size_bytes=4,
            sha256="",
            primary_url=primary_url,
            mirror_urls=[mirror_url],
        )

        with patch("openvoicy_sidecar.model_cache.download_file", side_effect=mock_download):
            with patch("openvoicy_sidecar.model_cache.log") as mock_log:
                download_with_mirrors(
                    file_info,
                    dest,
                    verify_callback=verify_download,
                )

        assert calls == [primary_url, mirror_url]
        assert dest.read_bytes() == b"good"
        success_logs = [c.args[0] for c in mock_log.call_args_list if c.args]
        assert any("Download succeeded from mirror 2/2" in entry for entry in success_logs)

    def test_download_all_mirrors_fail(self, tmp_path):
        """Should raise if all mirrors fail."""
        dest = tmp_path / "downloaded.bin"

        def mock_download(url, path, size, callback):
            raise NetworkError("Failed", url)

        file_info = ModelFileInfo(
            path="file.bin",
            size_bytes=100,
            sha256="",
            primary_url="http://primary.example.com/file.bin",
            mirror_urls=["http://mirror.example.com/file.bin"],
        )

        with patch("openvoicy_sidecar.model_cache.download_file", side_effect=mock_download):
            with patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_MAX_ATTEMPTS", 1):
                with pytest.raises(NetworkError):
                    download_with_mirrors(file_info, dest)

    def test_download_with_mirrors_retries_network_error_with_backoff(self, tmp_path):
        """Should retry network errors with exponential backoff before succeeding."""
        dest = tmp_path / "downloaded.bin"
        calls: list[str] = []

        def mock_download(url, path, size, callback):
            calls.append(url)
            if len(calls) < 3:
                raise NetworkError("temporary network issue", url)
            path.write_bytes(b"good")

        file_info = ModelFileInfo(
            path="file.bin",
            size_bytes=4,
            sha256="",
            primary_url="http://primary.example.com/file.bin",
            mirror_urls=[],
        )

        with (
            patch("openvoicy_sidecar.model_cache.download_file", side_effect=mock_download),
            patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_MAX_ATTEMPTS", 3),
            patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_INITIAL_BACKOFF_SECONDS", 0.25),
            patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_BACKOFF_MULTIPLIER", 2.0),
            patch("openvoicy_sidecar.model_cache.time.sleep") as mock_sleep,
        ):
            download_with_mirrors(file_info, dest)

        assert calls == [file_info.primary_url, file_info.primary_url, file_info.primary_url]
        assert dest.read_bytes() == b"good"
        assert [c.args[0] for c in mock_sleep.call_args_list] == [0.25, 0.5]

    def test_download_with_mirrors_raises_network_after_max_retries(self, tmp_path):
        """Should propagate E_NETWORK after retry budget is exhausted."""
        dest = tmp_path / "downloaded.bin"

        def mock_download(url, path, size, callback):
            raise NetworkError("network unavailable", url)

        file_info = ModelFileInfo(
            path="file.bin",
            size_bytes=4,
            sha256="",
            primary_url="http://primary.example.com/file.bin",
            mirror_urls=[],
        )

        with (
            patch("openvoicy_sidecar.model_cache.download_file", side_effect=mock_download),
            patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_MAX_ATTEMPTS", 3),
            patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_INITIAL_BACKOFF_SECONDS", 0.1),
            patch("openvoicy_sidecar.model_cache.NETWORK_RETRY_BACKOFF_MULTIPLIER", 2.0),
            patch("openvoicy_sidecar.model_cache.time.sleep") as mock_sleep,
        ):
            with pytest.raises(NetworkError) as exc_info:
                download_with_mirrors(file_info, dest)

        assert exc_info.value.code == "E_NETWORK"
        assert "network unavailable" in exc_info.value.message
        assert [c.args[0] for c in mock_sleep.call_args_list] == [0.1, 0.2]


# === Unit Tests: ModelCacheManager ===


class TestModelCacheManager:
    """Tests for ModelCacheManager."""

    def test_initial_status_missing(self, temp_cache_dir):
        """Should start with MISSING status."""
        manager = ModelCacheManager()
        assert manager.status == ModelStatus.MISSING

    def test_check_cache_empty(self, temp_cache_dir, sample_manifest):
        """Should return False for empty cache."""
        manager = ModelCacheManager()
        assert not manager.check_cache(sample_manifest)
        assert manager.status == ModelStatus.MISSING

    def test_check_cache_valid(self, temp_cache_dir, sample_manifest):
        """Should return True for valid cache."""
        manager = ModelCacheManager()

        # Create cached files
        model_dir = temp_cache_dir / sample_manifest.model_id
        model_dir.mkdir()

        # Create manifest
        (model_dir / "manifest.json").write_text(json.dumps({"model_id": "test"}))

        # Create model files with correct sizes
        (model_dir / "model.bin").write_bytes(b"x" * 800)
        (model_dir / "config.json").write_bytes(b"x" * 200)

        # Patch verify_file to skip actual hash check
        with patch("openvoicy_sidecar.model_cache.verify_file", return_value=True):
            assert manager.check_cache(sample_manifest)
            assert manager.status == ModelStatus.READY

    def test_get_status(self, temp_cache_dir, sample_manifest):
        """Should return status dictionary."""
        manager = ModelCacheManager()
        manager._manifest = sample_manifest

        status = manager.get_status(sample_manifest)

        assert status["model_id"] == "test-model"
        assert status["status"] == "missing"
        assert status["cache_path"] is not None

    def test_check_cache_lock_timeout_does_not_overwrite_status(
        self, temp_cache_dir, sample_manifest
    ):
        """Lock contention during status checks should not clobber in-flight state."""
        manager = ModelCacheManager()
        manager._status = ModelStatus.DOWNLOADING

        with patch(
            "openvoicy_sidecar.model_cache.CacheLock.__enter__",
            side_effect=LockError("Timeout waiting for cache lock"),
        ):
            assert manager.check_cache(sample_manifest) is False

        assert manager.status == ModelStatus.DOWNLOADING

    def test_purge_cache(self, temp_cache_dir, sample_manifest):
        """Should purge cache directory and return purged model IDs."""
        manager = ModelCacheManager()

        # Create some cached data
        model_dir = temp_cache_dir / sample_manifest.model_id
        model_dir.mkdir()
        (model_dir / "test.txt").write_text("test")

        assert model_dir.exists()

        purged = manager.purge_cache(sample_manifest.model_id)

        assert not model_dir.exists()
        assert manager.status == ModelStatus.MISSING
        assert purged == [sample_manifest.model_id]

    def test_purge_cache_all_returns_all_ids(self, temp_cache_dir):
        """Should return all purged model IDs when purging all."""
        manager = ModelCacheManager()

        for name in ["model-a", "model-b", "model-c"]:
            d = temp_cache_dir / name
            d.mkdir()
            (d / "data.bin").write_text("x")

        purged = manager.purge_cache(None)

        assert sorted(purged) == ["model-a", "model-b", "model-c"]
        assert manager.status == ModelStatus.MISSING

    def test_purge_cache_model_in_use(self, temp_cache_dir):
        """Should raise error if model in use."""
        manager = ModelCacheManager()
        manager.set_model_in_use(True)

        with pytest.raises(ModelInUseError):
            manager.purge_cache()

    @pytest.mark.parametrize(
        "invalid_model_id",
        ["", "   ", "../outside", "/tmp/unsafe", "model/../other", "C:/Windows/System32", "bad|id"],
    )
    def test_purge_cache_rejects_invalid_target_model_ids(self, temp_cache_dir, invalid_model_id):
        """Targeted purge must reject invalid/traversal model_id values."""
        manager = ModelCacheManager()

        with pytest.raises(ModelCacheError) as exc_info:
            manager.purge_cache(invalid_model_id)

        assert exc_info.value.code == "E_INVALID_PARAMS"


class TestModelPurgeHandler:
    def test_requires_string_model_id_when_provided(self):
        request = Request(method="model.purge_cache", id=1, params={"model_id": 123})

        with pytest.raises(ModelCacheError) as exc_info:
            handle_model_purge_cache(request)

        assert exc_info.value.code == "E_INVALID_PARAMS"


# === Unit Tests: DownloadProgress ===


class TestDownloadProgress:
    """Tests for DownloadProgress."""

    def test_to_dict(self):
        """Should convert to dictionary."""
        progress = DownloadProgress(
            current_bytes=500,
            total_bytes=1000,
            current_file="model.bin",
            files_completed=1,
            files_total=3,
        )

        d = progress.to_dict()

        assert d["current"] == 500
        assert d["total"] == 1000
        assert d["unit"] == "bytes"
        assert d["current_file"] == "model.bin"

    def test_to_dict_unknown_total(self):
        """Should handle unknown total."""
        progress = DownloadProgress(current_bytes=500, total_bytes=0)

        d = progress.to_dict()

        assert d["current"] == 500
        assert d["total"] is None


# === Integration Tests ===


class TestModelCacheIntegration:
    """Integration tests for model cache."""

    def test_full_download_flow_mock(self, temp_cache_dir, sample_manifest):
        """Should complete full download flow with mocks."""
        manager = ModelCacheManager()

        def mock_download(file_info, dest, callback):
            # Simulate successful download
            if file_info.path == "model.bin":
                dest.write_bytes(b"x" * 800)
            else:
                dest.write_bytes(b"x" * 200)

        # Patch verify to always succeed (skip hash check)
        with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=mock_download):
            with patch("openvoicy_sidecar.model_cache.verify_file", return_value=True):
                with patch(
                    "openvoicy_sidecar.model_cache.verify_sha256",
                    return_value=(True, sample_manifest.files[0].sha256),
                ):
                    model_dir = manager.download_model(sample_manifest)

        assert model_dir.exists()
        assert (model_dir / "manifest.json").exists()
        assert (model_dir / "model.bin").exists()
        assert manager.status == ModelStatus.READY

    def test_download_corrupt_file_fails(self, temp_cache_dir, sample_manifest):
        """Should fail if verification fails."""
        manager = ModelCacheManager()

        def mock_download(file_info, dest, callback):
            dest.write_bytes(b"corrupt")

        with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=mock_download):
            with pytest.raises(CacheCorruptError) as exc_info:
                manager.download_model(sample_manifest)

        error = exc_info.value
        assert error.code == "E_CACHE_CORRUPT"
        assert error.details["expected_sha256"] == sample_manifest.files[0].sha256
        assert error.details["actual_sha256"]
        assert "purge" in error.details["suggested_recovery"].lower()
        assert error.details["recoverable"] is True
        assert not (temp_cache_dir / ".partial" / sample_manifest.model_id / "model.bin").exists()
        assert manager.status == ModelStatus.ERROR
        assert manager.error is not None
        assert "hash mismatch" in manager.error.lower()

    def test_download_model_retries_next_mirror_on_hash_mismatch(self, temp_cache_dir):
        """Should fallback to next mirror when first URL yields hash mismatch."""
        manager = ModelCacheManager()
        primary_url = "http://primary.example.com/model.bin"
        mirror_url = "http://mirror.example.com/model.bin"

        manifest = ModelManifest(
            model_id="mirror-retry-model",
            revision="r1",
            display_name="Mirror Retry Model",
            total_size_bytes=4,
            files=[
                ModelFileInfo(
                    path="model.bin",
                    size_bytes=4,
                    sha256=hashlib.sha256(b"good").hexdigest(),
                    primary_url=primary_url,
                    mirror_urls=[mirror_url],
                )
            ],
        )

        calls: list[str] = []

        def mock_download(url, dest, size, callback):
            calls.append(url)
            if url == primary_url:
                dest.write_bytes(b"bad!")
            else:
                dest.write_bytes(b"good")

        with patch("openvoicy_sidecar.model_cache.download_file", side_effect=mock_download):
            model_dir = manager.download_model(manifest)

        assert calls == [primary_url, mirror_url]
        assert model_dir == temp_cache_dir / manifest.model_id
        assert (model_dir / "model.bin").read_bytes() == b"good"
        assert manager.status == ModelStatus.READY

    def test_download_model_uses_cached_model_when_network_unavailable(
        self,
        temp_cache_dir,
        sample_manifest,
    ):
        """Offline mode: valid installed model should bypass network download."""
        manager = ModelCacheManager()

        model_dir = temp_cache_dir / sample_manifest.model_id
        model_dir.mkdir(parents=True)
        (model_dir / "manifest.json").write_text(json.dumps({"model_id": sample_manifest.model_id}))
        (model_dir / "model.bin").write_bytes(b"x" * 800)
        (model_dir / "config.json").write_bytes(b"y" * 200)

        with (
            patch("openvoicy_sidecar.model_cache.verify_file", return_value=True),
            patch("openvoicy_sidecar.model_cache.download_with_mirrors") as mock_download,
        ):
            resolved = manager.download_model(sample_manifest)

        assert resolved == model_dir
        assert manager.status == ModelStatus.READY
        mock_download.assert_not_called()

    def test_download_model_cleans_partial_dir_on_disk_full(self, temp_cache_dir, sample_manifest):
        """Should clear staged .partial data when a disk-full error occurs."""
        manager = ModelCacheManager()

        def fail_with_disk_full(_file_info, _dest, _callback):
            raise DiskFullError(
                1234,
                321,
                "Insufficient disk space while downloading model files. "
                "required_bytes=1234, available_bytes=321. "
                "Run purge_model_cache to free space, then retry.",
            )

        with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=fail_with_disk_full):
            with pytest.raises(DiskFullError):
                manager.download_model(sample_manifest)

        partial_dir = temp_cache_dir / ".partial" / sample_manifest.model_id
        assert not partial_dir.exists()
        assert manager.status == ModelStatus.ERROR
        assert manager.error is not None
        assert "purge_model_cache" in manager.error

    def test_concurrent_downloads_blocked(self, temp_cache_dir, sample_manifest):
        """Should block concurrent downloads with lock."""
        manager1 = ModelCacheManager()
        manager2 = ModelCacheManager()

        results = {"manager1": None, "manager2": None}

        def slow_download(file_info, dest, callback):
            time.sleep(1)  # Simulate slow download
            dest.write_bytes(b"x" * file_info.size_bytes)

        def download1():
            with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=slow_download):
                with patch("openvoicy_sidecar.model_cache.verify_file", return_value=True):
                    with patch(
                        "openvoicy_sidecar.model_cache.verify_sha256",
                        return_value=(True, sample_manifest.files[0].sha256),
                    ):
                        try:
                            manager1.download_model(sample_manifest)
                            results["manager1"] = "success"
                        except Exception as e:
                            results["manager1"] = f"error: {e}"

        def download2():
            time.sleep(0.1)  # Start slightly after manager1
            # This should wait for lock
            with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=slow_download):
                with patch("openvoicy_sidecar.model_cache.verify_file", return_value=True):
                    with patch(
                        "openvoicy_sidecar.model_cache.verify_sha256",
                        return_value=(True, sample_manifest.files[0].sha256),
                    ):
                        try:
                            manager2.download_model(sample_manifest)
                            results["manager2"] = "success"
                        except Exception as e:
                            results["manager2"] = f"error: {e}"

        t1 = threading.Thread(target=download1)
        t2 = threading.Thread(target=download2)

        t1.start()
        t2.start()

        t1.join(timeout=10)
        t2.join(timeout=10)

        # Both should succeed (second one waits for lock)
        # OR second finds cache already populated
        assert "success" in results["manager1"] or "error" not in results["manager1"]

    def test_interrupted_download_keeps_partial_staging(self, temp_cache_dir, sample_manifest):
        """Should preserve .partial staging on interruption for resume support."""
        manager = ModelCacheManager()

        def interrupted_download(file_info, dest, callback):
            dest.write_bytes(b"x" * min(file_info.size_bytes, 128))
            raise NetworkError("connection dropped", file_info.primary_url)

        with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=interrupted_download):
            with pytest.raises(NetworkError):
                manager.download_model(sample_manifest)

        partial_dir = temp_cache_dir / ".partial" / sample_manifest.model_id
        assert partial_dir.exists()
        assert (partial_dir / "manifest.json").exists()
        assert (partial_dir / "model.bin").exists()

    def test_resume_reuses_verified_staged_files(self, temp_cache_dir):
        """Should skip re-downloading files already valid in .partial staging."""
        manager = ModelCacheManager()
        manifest = ModelManifest(
            model_id="resume-model",
            revision="r1",
            display_name="Resume Model",
            total_size_bytes=7,
            files=[
                ModelFileInfo(
                    path="model.bin",
                    size_bytes=4,
                    sha256="",
                    primary_url="http://example.com/model.bin",
                ),
                ModelFileInfo(
                    path="config.json",
                    size_bytes=3,
                    sha256="",
                    primary_url="http://example.com/config.json",
                ),
            ],
        )

        partial_dir = temp_cache_dir / ".partial" / manifest.model_id
        partial_dir.mkdir(parents=True)
        (partial_dir / "model.bin").write_bytes(b"xxxx")

        downloaded_paths: list[str] = []

        def mock_download(file_info, dest, callback):
            downloaded_paths.append(file_info.path)
            if file_info.path == "config.json":
                dest.write_bytes(b"cfg")
            else:
                dest.write_bytes(b"xxxx")

        with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=mock_download):
            model_dir = manager.download_model(manifest)

        assert downloaded_paths == ["config.json"]
        assert model_dir == temp_cache_dir / manifest.model_id
        assert (model_dir / "model.bin").read_bytes() == b"xxxx"
        assert (model_dir / "config.json").read_bytes() == b"cfg"
        assert not partial_dir.exists()

    def test_download_model_activates_staging_via_os_rename(self, temp_cache_dir, sample_manifest):
        """Should atomically activate .partial staging with os.rename."""
        manager = ModelCacheManager()

        def mock_download(file_info, dest, callback):
            if file_info.path == "model.bin":
                dest.write_bytes(b"x" * 800)
            else:
                dest.write_bytes(b"x" * 200)

        original_rename = os.rename
        with patch("openvoicy_sidecar.model_cache.download_with_mirrors", side_effect=mock_download):
            with patch(
                "openvoicy_sidecar.model_cache.verify_sha256",
                return_value=(True, sample_manifest.files[0].sha256),
            ):
                with patch("openvoicy_sidecar.model_cache.os.rename", wraps=original_rename) as spy_rename:
                    model_dir = manager.download_model(sample_manifest)

        expected_staged = temp_cache_dir / ".partial" / sample_manifest.model_id
        expected_final = temp_cache_dir / sample_manifest.model_id
        assert model_dir == expected_final
        assert any(
            Path(call.args[0]) == expected_staged and Path(call.args[1]) == expected_final
            for call in spy_rename.call_args_list
        )


class _FakeThread:
    def __init__(self, target=None, args=(), daemon=False):
        self._target = target
        self._args = args
        self.daemon = daemon
        self.started = False
        self._alive = True

    def start(self):
        self.started = True
        # Do not execute target; keep deterministic for handler tests.

    def is_alive(self):
        return self._alive


class _AliveThread:
    def is_alive(self):
        return True


class TestModelInstallHandler:
    @pytest.fixture(autouse=True)
    def _reset_install_globals(self):
        model_cache._install_thread = None
        model_cache._install_model_id = None
        model_cache._install_revision = None
        yield
        model_cache._install_thread = None
        model_cache._install_model_id = None
        model_cache._install_revision = None

    def test_requires_model_id(self):
        request = Request(method="model.install", id=1, params={})

        with pytest.raises(ModelCacheError) as exc_info:
            handle_model_install(request)

        assert exc_info.value.code == "E_INVALID_PARAMS"

    def test_resolves_manifest_from_catalog(self, tmp_path):
        shared = tmp_path / "shared"
        model_dir = shared / "model"
        manifests = model_dir / "manifests"
        manifests.mkdir(parents=True)
        manifest_path = manifests / "parakeet-tdt-0.6b-v3.json"
        manifest_path.write_text('{"model_id":"parakeet-tdt-0.6b-v3","revision":"r1","files":[]}')
        (model_dir / "MODEL_CATALOG.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "models": [
                        {
                            "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                            "manifest_path": "manifests/parakeet-tdt-0.6b-v3.json",
                        }
                    ],
                }
            )
        )

        with patch.dict(os.environ, {"OPENVOICY_SHARED_ROOT": str(shared)}):
            resolved = model_cache._resolve_manifest_path_for_model("nvidia/parakeet-tdt-0.6b-v3")

        assert resolved == manifest_path

    def test_returns_installing_and_starts_background_thread(self):
        manifest = ModelManifest(
            model_id="parakeet-tdt-0.6b-v3",
            revision="rev-1",
            display_name="Parakeet",
            total_size_bytes=123,
            files=[],
        )

        manager = MagicMock()
        manager.load_manifest.return_value = manifest
        manager.progress.to_dict.return_value = {
            "current": 0,
            "total": 123,
            "unit": "bytes",
            "current_file": "",
            "files_completed": 0,
            "files_total": 0,
        }

        with (
            patch("openvoicy_sidecar.model_cache._resolve_manifest_path_for_model", return_value=Path("/tmp/manifest.json")),
            patch("openvoicy_sidecar.model_cache.get_cache_manager", return_value=manager),
            patch("openvoicy_sidecar.model_cache.threading.Thread", _FakeThread),
        ):
            # Ensure deterministic global state for this test.
            model_cache._install_thread = None
            model_cache._install_model_id = None
            model_cache._install_revision = None

            request = Request(
                method="model.install",
                id=1,
                params={"model_id": "nvidia/parakeet-tdt-0.6b-v3"},
            )
            result = handle_model_install(request)

        assert result["status"] == "installing"
        assert result["model_id"] == "parakeet-tdt-0.6b-v3"
        assert result["revision"] == "rev-1"
        assert model_cache._install_thread is not None
        assert model_cache._install_thread.started is True

    def test_idempotent_when_already_installing(self):
        manifest = ModelManifest(
            model_id="parakeet-tdt-0.6b-v3",
            revision="rev-2",
            display_name="Parakeet",
            total_size_bytes=321,
            files=[],
        )
        manager = MagicMock()
        manager.load_manifest.return_value = manifest
        manager.progress.to_dict.return_value = {
            "current": 42,
            "total": 321,
            "unit": "bytes",
            "current_file": "model.nemo",
            "files_completed": 0,
            "files_total": 1,
        }

        with (
            patch("openvoicy_sidecar.model_cache._resolve_manifest_path_for_model", return_value=Path("/tmp/manifest.json")),
            patch("openvoicy_sidecar.model_cache.get_cache_manager", return_value=manager),
        ):
            model_cache._install_thread = _AliveThread()
            model_cache._install_model_id = "parakeet-tdt-0.6b-v3"
            model_cache._install_revision = "rev-2"

            request = Request(
                method="model.install",
                id=2,
                params={"model_id": "nvidia/parakeet-tdt-0.6b-v3"},
            )
            result = handle_model_install(request)

        assert result["status"] == "installing"
        assert result["model_id"] == "parakeet-tdt-0.6b-v3"
        assert "progress" in result
        assert result["progress"]["current"] == 42

    def test_idempotent_when_different_model_requested_returns_active_install_metadata(self):
        requested_manifest = ModelManifest(
            model_id="other-model",
            revision="rev-other",
            display_name="Other Model",
            total_size_bytes=321,
            files=[],
        )
        manager = MagicMock()
        manager.load_manifest.return_value = requested_manifest
        manager.progress.to_dict.return_value = {
            "current": 42,
            "total": 321,
            "unit": "bytes",
            "current_file": "model.nemo",
            "files_completed": 0,
            "files_total": 1,
        }

        with (
            patch(
                "openvoicy_sidecar.model_cache._resolve_manifest_path_for_model",
                return_value=Path("/tmp/manifest.json"),
            ),
            patch("openvoicy_sidecar.model_cache.get_cache_manager", return_value=manager),
        ):
            model_cache._install_thread = _AliveThread()
            model_cache._install_model_id = "parakeet-tdt-0.6b-v3"
            model_cache._install_revision = "rev-active"

            request = Request(
                method="model.install",
                id=3,
                params={"model_id": "nvidia/other-model"},
            )
            result = handle_model_install(request)

        assert result["status"] == "installing"
        assert result["model_id"] == "parakeet-tdt-0.6b-v3"
        assert result["revision"] == "rev-active"
        assert "progress" in result


class TestModelProgressEmitter:
    def test_progress_stage_mapping(self):
        assert model_cache._progress_stage_for_status(ModelStatus.DOWNLOADING) == "downloading"
        assert model_cache._progress_stage_for_status(ModelStatus.VERIFYING) == "verifying"
        assert model_cache._progress_stage_for_status(ModelStatus.READY) == "installing"

    def test_emits_on_stage_change_without_waiting_for_interval(self):
        emitter = model_cache._ModelProgressEmitter("model-id")
        progress = DownloadProgress(
            current_bytes=64,
            total_bytes=100,
            current_file="model.bin",
            files_completed=0,
            files_total=1,
        )

        with (
            patch("openvoicy_sidecar.model_cache._emit_model_progress") as mock_emit,
            patch(
                "openvoicy_sidecar.model_cache.time.monotonic",
                side_effect=[0.0, 0.1, 0.2],
            ),
        ):
            emitter.emit(progress, stage="downloading")
            emitter.emit(progress, stage="downloading")
            emitter.emit(progress, stage="verifying")

        assert mock_emit.call_count == 2
        assert mock_emit.call_args_list[0].kwargs["stage"] == "downloading"
        assert mock_emit.call_args_list[1].kwargs["stage"] == "verifying"

    def test_emits_on_percent_step_or_interval(self):
        emitter = model_cache._ModelProgressEmitter("model-id")
        base = DownloadProgress(
            current_bytes=0,
            total_bytes=100,
            current_file="",
            files_completed=0,
            files_total=1,
        )
        percent_advanced = DownloadProgress(
            current_bytes=1,
            total_bytes=100,
            current_file="",
            files_completed=0,
            files_total=1,
        )

        with (
            patch("openvoicy_sidecar.model_cache._emit_model_progress") as mock_emit,
            patch(
                "openvoicy_sidecar.model_cache.time.monotonic",
                side_effect=[0.0, 0.2, 0.4, 1.5],
            ),
        ):
            emitter.emit(base, stage="downloading")
            emitter.emit(base, stage="downloading")
            emitter.emit(percent_advanced, stage="downloading")
            emitter.emit(percent_advanced, stage="downloading")

        assert mock_emit.call_count == 3
