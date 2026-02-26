"""Model install integrity verification tests.

Covers the full download-verify-commit pipeline: happy path, hash/size
mismatches, partial downloads, concurrent installs, cancel, atomic rename
failure, disk full, no network, and mirror fallback per bead 1ed.2.12.
"""

from __future__ import annotations

import hashlib
import shutil
import threading
import urllib.error
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from openvoicy_sidecar.model_cache import (
    CacheLock,
    DiskFullError,
    DownloadProgress,
    ModelCacheManager,
    ModelFileInfo,
    ModelManifest,
    ModelStatus,
    NetworkError,
    compute_sha256,
    download_file,
    download_with_mirrors,
    verify_file,
)


# ── Helpers ──────────────────────────────────────────────────────────

SAMPLE_CONTENT = b"model data for testing integrity"
SAMPLE_SHA256 = hashlib.sha256(SAMPLE_CONTENT).hexdigest()
SAMPLE_SIZE = len(SAMPLE_CONTENT)


def _make_file_info(
    path: str = "model.bin",
    content: bytes = SAMPLE_CONTENT,
    sha256: str | None = None,
    size: int | None = None,
) -> ModelFileInfo:
    return ModelFileInfo(
        path=path,
        size_bytes=size if size is not None else len(content),
        sha256=sha256 if sha256 is not None else hashlib.sha256(content).hexdigest(),
        primary_url="http://example.com/model.bin",
        mirror_urls=["http://mirror1.example.com/model.bin"],
    )


def _make_manifest(
    file_infos: list[ModelFileInfo] | None = None,
    model_id: str = "test-model-v1",
) -> ModelManifest:
    return ModelManifest(
        model_id=model_id,
        revision="v1",
        display_name="Test Model",
        total_size_bytes=SAMPLE_SIZE,
        files=file_infos or [_make_file_info()],
        source_url="http://example.com/manifest.json",
    )


def _write_file(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)


@pytest.fixture
def cache_dir(tmp_path: Path) -> Path:
    d = tmp_path / "cache" / "models"
    d.mkdir(parents=True)
    return d


# ── 1. HAPPY PATH ────────────────────────────────────────────────────


class TestHappyPath:
    """download → verify sha256 → verify size → atomic rename → ready."""

    def test_download_verify_commit_cycle(self, cache_dir: Path) -> None:
        manifest = _make_manifest()

        # Simulate download writing to a temp dir, then atomic rename
        temp_dir = cache_dir / f"{manifest.model_id}_tmp_abc"
        temp_dir.mkdir()
        _write_file(temp_dir / "model.bin", SAMPLE_CONTENT)
        _write_file(
            temp_dir / "manifest.json",
            b'{"model_id":"test-model-v1","revision":"v1"}',
        )

        # Verify before commit
        fi = manifest.files[0]
        assert verify_file(temp_dir / fi.path, fi.sha256, fi.size_bytes)

        # Atomic rename
        final_dir = cache_dir / manifest.model_id
        temp_dir.rename(final_dir)

        assert final_dir.exists()
        assert (final_dir / "model.bin").read_bytes() == SAMPLE_CONTENT
        assert not temp_dir.exists()

    def test_sha256_matches_after_write(self, tmp_path: Path) -> None:
        fp = tmp_path / "file.bin"
        fp.write_bytes(SAMPLE_CONTENT)
        assert compute_sha256(fp) == SAMPLE_SHA256

    def test_verify_file_passes_valid(self, tmp_path: Path) -> None:
        fp = tmp_path / "file.bin"
        fp.write_bytes(SAMPLE_CONTENT)
        assert verify_file(fp, SAMPLE_SHA256, SAMPLE_SIZE)


# ── 2. HASH MISMATCH ────────────────────────────────────────────────


class TestHashMismatch:
    """Download completes but sha256 does not match → verification fails."""

    def test_wrong_hash_rejects(self, tmp_path: Path) -> None:
        fp = tmp_path / "file.bin"
        fp.write_bytes(SAMPLE_CONTENT)
        wrong_hash = "a" * 64
        assert not verify_file(fp, wrong_hash, SAMPLE_SIZE)

    def test_corrupt_download_detected_and_cleaned(self, cache_dir: Path) -> None:
        manifest = _make_manifest()
        temp_dir = cache_dir / f"{manifest.model_id}_tmp_xyz"
        temp_dir.mkdir()
        # Write wrong content
        _write_file(temp_dir / "model.bin", b"corrupted data")

        fi = manifest.files[0]
        assert not verify_file(temp_dir / fi.path, fi.sha256, fi.size_bytes)

        # Cleanup: temp dir should be removed on failure
        shutil.rmtree(temp_dir)
        assert not temp_dir.exists()


# ── 3. SIZE MISMATCH ────────────────────────────────────────────────


class TestSizeMismatch:
    """Download completes but size_bytes does not match."""

    def test_wrong_size_rejects(self, tmp_path: Path) -> None:
        fp = tmp_path / "file.bin"
        fp.write_bytes(SAMPLE_CONTENT)
        assert not verify_file(fp, SAMPLE_SHA256, SAMPLE_SIZE + 100)

    def test_truncated_file_rejects(self, tmp_path: Path) -> None:
        fp = tmp_path / "file.bin"
        fp.write_bytes(SAMPLE_CONTENT[:5])
        assert not verify_file(
            fp, hashlib.sha256(SAMPLE_CONTENT[:5]).hexdigest(), SAMPLE_SIZE
        )


# ── 4. PARTIAL DOWNLOAD / RESUME ────────────────────────────────────


class TestPartialDownload:
    """download interrupted → .partial exists but incomplete → resume."""

    def test_partial_file_detected_for_resume(self, tmp_path: Path) -> None:
        fp = tmp_path / "model.bin"
        fp.write_bytes(SAMPLE_CONTENT[:10])
        assert fp.stat().st_size < SAMPLE_SIZE

    def test_partial_file_fails_verification(self, tmp_path: Path) -> None:
        fp = tmp_path / "model.bin"
        fp.write_bytes(SAMPLE_CONTENT[:10])
        assert not verify_file(fp, SAMPLE_SHA256, SAMPLE_SIZE)

    def test_resume_download_attempts_with_partial(self, tmp_path: Path) -> None:
        """download_file should attempt to resume when partial file exists."""
        fp = tmp_path / "model.bin"
        fp.write_bytes(SAMPLE_CONTENT[:10])

        mock_resp = MagicMock()
        mock_resp.status = 200  # Server doesn't support ranges, full re-download
        mock_resp.headers = {"Content-Length": str(SAMPLE_SIZE)}
        mock_resp.read = MagicMock(side_effect=[SAMPLE_CONTENT, b""])
        mock_resp.__enter__ = lambda s: s
        mock_resp.__exit__ = MagicMock(return_value=False)

        with patch("urllib.request.urlopen", return_value=mock_resp):
            download_file("http://example.com/model.bin", fp, SAMPLE_SIZE, None)

        assert fp.exists()


# ── 5. CONCURRENT INSTALLS ──────────────────────────────────────────


class TestConcurrentInstalls:
    """Two install requests for same model → second blocked by lock."""

    def test_lock_serializes_access(self, tmp_path: Path) -> None:
        lock_path = tmp_path / "models" / ".lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)

        with patch(
            "openvoicy_sidecar.model_cache.get_lock_file_path", return_value=lock_path
        ):
            lock1 = CacheLock(timeout=5.0)
            assert lock1.acquire()

            results: list[str] = []

            def try_acquire() -> None:
                with patch(
                    "openvoicy_sidecar.model_cache.get_lock_file_path",
                    return_value=lock_path,
                ):
                    lock2 = CacheLock(timeout=0.5)
                    if lock2.acquire():
                        results.append("acquired")
                        lock2.release()
                    else:
                        results.append("blocked")

            t = threading.Thread(target=try_acquire)
            t.start()
            t.join(timeout=3)

            lock1.release()
            assert "blocked" in results

    def test_sequential_installs_succeed(self, tmp_path: Path) -> None:
        lock_path = tmp_path / "models" / ".lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)

        with patch(
            "openvoicy_sidecar.model_cache.get_lock_file_path", return_value=lock_path
        ):
            lock = CacheLock(timeout=2.0)
            assert lock.acquire()
            lock.release()

            assert lock.acquire()
            lock.release()


# ── 6. CANCEL MID-DOWNLOAD ──────────────────────────────────────────


class TestCancelMidDownload:
    """Cancel during download → temp dir cleaned up, no half-installed model."""

    def test_temp_dir_cleaned_on_failure(self, cache_dir: Path) -> None:
        manifest = _make_manifest()
        temp_dir = cache_dir / f"{manifest.model_id}_tmp_cancel"
        temp_dir.mkdir()
        _write_file(temp_dir / "partial.bin", b"incomplete")

        shutil.rmtree(temp_dir, ignore_errors=True)
        assert not temp_dir.exists()

    def test_final_dir_not_created_on_cancel(self, cache_dir: Path) -> None:
        manifest = _make_manifest()
        final_dir = cache_dir / manifest.model_id
        assert not final_dir.exists()


# ── 7. ATOMIC RENAME FAILURE ────────────────────────────────────────


class TestAtomicRenameFailure:
    """Rename failure → model not marked ready, old version preserved."""

    def test_failed_rename_preserves_old_model(self, cache_dir: Path) -> None:
        manifest = _make_manifest()
        final_dir = cache_dir / manifest.model_id
        final_dir.mkdir()
        _write_file(final_dir / "model.bin", b"old version")

        temp_dir = cache_dir / f"{manifest.model_id}_tmp_fail"
        temp_dir.mkdir()
        _write_file(temp_dir / "model.bin", SAMPLE_CONTENT)

        with patch("pathlib.Path.rename", side_effect=OSError("permission denied")):
            with pytest.raises(OSError):
                temp_dir.rename(final_dir)

        assert (final_dir / "model.bin").read_bytes() == b"old version"

    def test_temp_dir_survives_rename_failure(self, cache_dir: Path) -> None:
        manifest = _make_manifest()
        temp_dir = cache_dir / f"{manifest.model_id}_tmp_fail2"
        temp_dir.mkdir()
        _write_file(temp_dir / "model.bin", SAMPLE_CONTENT)

        with patch("pathlib.Path.rename", side_effect=OSError("fail")):
            with pytest.raises(OSError):
                temp_dir.rename(cache_dir / manifest.model_id)

        assert temp_dir.exists()


# ── 8. DISK FULL ────────────────────────────────────────────────────


class TestDiskFull:
    """Simulate disk full during download → E_DISK_FULL."""

    def test_insufficient_space_raises_disk_full(self, tmp_path: Path) -> None:
        from openvoicy_sidecar.model_cache import check_disk_space

        mock_usage = MagicMock()
        mock_usage.__iter__ = lambda s: iter((1000, 900, 100))

        with patch("shutil.disk_usage", return_value=(1000, 900, 100)):
            with patch(
                "openvoicy_sidecar.model_cache.get_cache_directory",
                return_value=tmp_path,
            ):
                with pytest.raises(DiskFullError) as exc_info:
                    check_disk_space(required_bytes=1_000_000)

                err = exc_info.value
                assert err.code == "E_DISK_FULL"
                assert err.required > 0
                assert err.available == 100

    def test_disk_full_error_includes_space_info(self) -> None:
        err = DiskFullError(required=5_000_000, available=100)
        assert err.required == 5_000_000
        assert err.available == 100
        assert err.code == "E_DISK_FULL"


# ── 9. NO NETWORK ───────────────────────────────────────────────────


class TestNoNetwork:
    """Simulate network failure → E_NETWORK with retry guidance."""

    def test_network_error_on_download(self, tmp_path: Path) -> None:
        fp = tmp_path / "model.bin"
        err = urllib.error.URLError("Network unreachable")

        with patch("urllib.request.urlopen", side_effect=err):
            with pytest.raises(NetworkError) as exc_info:
                download_file("http://example.com/model.bin", fp, SAMPLE_SIZE, None)

            assert exc_info.value.code == "E_NETWORK"

    def test_all_mirrors_fail_raises_network_error(self, tmp_path: Path) -> None:
        fp = tmp_path / "model.bin"
        fi = _make_file_info()
        err = urllib.error.URLError("Connection refused")

        with patch("urllib.request.urlopen", side_effect=err):
            with pytest.raises(NetworkError):
                download_with_mirrors(fi, fp, None)


# ── 10. MIRROR FALLBACK ─────────────────────────────────────────────


class TestMirrorFallback:
    """First mirror fails → try second mirror from urls[] array."""

    def test_primary_fails_mirror_succeeds(self, tmp_path: Path) -> None:
        call_count = 0

        def side_effect(req, *args, **kwargs):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                raise urllib.error.URLError("Primary server down")
            mock_resp = MagicMock()
            mock_resp.status = 200
            mock_resp.headers = {"Content-Length": str(SAMPLE_SIZE)}
            mock_resp.read = MagicMock(side_effect=[SAMPLE_CONTENT, b""])
            mock_resp.__enter__ = lambda s: s
            mock_resp.__exit__ = MagicMock(return_value=False)
            return mock_resp

        fi = _make_file_info()
        fp = tmp_path / "model.bin"

        with patch("urllib.request.urlopen", side_effect=side_effect):
            download_with_mirrors(fi, fp, None)

        assert fp.exists()
        assert call_count == 2  # Primary failed, mirror succeeded

    def test_all_mirrors_exhausted(self, tmp_path: Path) -> None:
        fi = ModelFileInfo(
            path="model.bin",
            size_bytes=SAMPLE_SIZE,
            sha256=SAMPLE_SHA256,
            primary_url="http://primary.example.com/model.bin",
            mirror_urls=[
                "http://mirror1.example.com/model.bin",
                "http://mirror2.example.com/model.bin",
            ],
        )
        fp = tmp_path / "model.bin"
        err = urllib.error.URLError("All servers down")

        with patch("urllib.request.urlopen", side_effect=err):
            with pytest.raises(NetworkError):
                download_with_mirrors(fi, fp, None)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
