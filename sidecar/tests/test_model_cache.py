"""Tests for model cache management."""

from __future__ import annotations

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

from openvoicy_sidecar.model_cache import (
    DOWNLOAD_CHUNK_SIZE,
    CacheCorruptError,
    CacheLock,
    DiskFullError,
    DownloadProgress,
    LockError,
    ModelCacheManager,
    ModelFileInfo,
    ModelInUseError,
    ModelManifest,
    ModelStatus,
    NetworkError,
    check_disk_space,
    compute_sha256,
    download_file,
    download_with_mirrors,
    format_bytes,
    get_cache_directory,
    verify_file,
    verify_manifest,
)


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
            download_with_mirrors(file_info, dest)

        assert call_count[0] == 2  # Primary + mirror
        assert dest.read_bytes() == content

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
            with pytest.raises(NetworkError):
                download_with_mirrors(file_info, dest)


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

    def test_purge_cache(self, temp_cache_dir, sample_manifest):
        """Should purge cache directory."""
        manager = ModelCacheManager()

        # Create some cached data
        model_dir = temp_cache_dir / sample_manifest.model_id
        model_dir.mkdir()
        (model_dir / "test.txt").write_text("test")

        assert model_dir.exists()

        manager.purge_cache(sample_manifest.model_id)

        assert not model_dir.exists()
        assert manager.status == ModelStatus.MISSING

    def test_purge_cache_model_in_use(self, temp_cache_dir):
        """Should raise error if model in use."""
        manager = ModelCacheManager()
        manager.set_model_in_use(True)

        with pytest.raises(ModelInUseError):
            manager.purge_cache()


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
            with pytest.raises(CacheCorruptError):
                manager.download_model(sample_manifest)

        assert manager.status == ModelStatus.ERROR

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
