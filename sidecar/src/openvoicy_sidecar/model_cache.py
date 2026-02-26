"""Model cache management: download, verify, and purge.

This module provides robust model cache management with:
- Atomic downloads (temp → verify → rename)
- Resumable downloads via HTTP Range
- Mirror fallback
- Disk space preflight
- SHA-256 verification
- File locking for concurrent access

Cache Directory Structure:
~/.cache/openvoicy/models/
  parakeet-tdt-0.6b-v3/
    manifest.json
    parakeet-tdt-0.6b-v3.nemo
    README.md
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import threading
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, Callable, Optional
from urllib.parse import urlparse

from .protocol import Notification, Request, log, write_notification
from .resources import (
    MODEL_CATALOG_REL,
    MODEL_MANIFESTS_DIR_REL,
    MODEL_MANIFEST_REL,
    resolve_shared_path,
    resolve_shared_path_optional,
)

# === Constants ===

LOCK_TIMEOUT_SECONDS = 600  # 10 minutes for slow downloads
# Keep status-path cache checks responsive when another operation holds the lock.
CHECK_CACHE_LOCK_TIMEOUT_SECONDS = 1.0
DOWNLOAD_CHUNK_SIZE = 8192
DISK_SPACE_BUFFER = 1.1  # 10% buffer

TRUSTED_HF_HOSTS = ("huggingface.co", "hf.co")


class ModelStatus(Enum):
    """Model download/verification status."""

    MISSING = "missing"
    DOWNLOADING = "downloading"
    VERIFYING = "verifying"
    READY = "ready"
    ERROR = "error"


@dataclass
class DownloadProgress:
    """Progress information for downloads."""

    current_bytes: int = 0
    total_bytes: int = 0
    current_file: str = ""
    files_completed: int = 0
    files_total: int = 0

    def to_dict(self) -> dict[str, Any]:
        """Convert to API response format."""
        return {
            "current": self.current_bytes,
            "total": self.total_bytes if self.total_bytes > 0 else None,
            "unit": "bytes",
            "current_file": self.current_file,
            "files_completed": self.files_completed,
            "files_total": self.files_total,
        }


@dataclass
class ModelFileInfo:
    """Information about a single model file."""

    path: str
    size_bytes: int
    sha256: str
    description: str = ""
    primary_url: str = ""
    mirror_urls: list[str] = field(default_factory=list)


@dataclass
class ModelManifest:
    """Model manifest containing file information."""

    model_id: str
    revision: str
    display_name: str
    total_size_bytes: int
    files: list[ModelFileInfo]
    source_url: str = ""

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ModelManifest:
        """Create from dictionary."""
        files = []
        base_url = ""

        # Get base URL from mirrors if available
        mirrors = data.get("mirrors", [])
        if mirrors:
            # Extract base URL from first mirror
            first_mirror_url = mirrors[0].get("url", "")
            if first_mirror_url:
                # Get directory portion of URL
                parsed = urlparse(first_mirror_url)
                path_parts = parsed.path.rsplit("/", 1)
                if len(path_parts) > 1:
                    base_url = f"{parsed.scheme}://{parsed.netloc}{path_parts[0]}"

        for file_data in data.get("files", []):
            file_path = file_data.get("path", "")
            primary_url = f"{base_url}/{file_path}" if base_url else ""

            # Get mirror URLs for this file
            mirror_urls = []
            for mirror in mirrors:
                mirror_base = mirror.get("url", "")
                if mirror_base:
                    # Replace the filename in the mirror URL
                    parsed = urlparse(mirror_base)
                    path_parts = parsed.path.rsplit("/", 1)
                    if len(path_parts) > 1:
                        mirror_file_url = f"{parsed.scheme}://{parsed.netloc}{path_parts[0]}/{file_path}"
                        if mirror_file_url != primary_url:
                            mirror_urls.append(mirror_file_url)

            files.append(
                ModelFileInfo(
                    path=file_path,
                    size_bytes=file_data.get("size_bytes", 0),
                    sha256=file_data.get("sha256", ""),
                    description=file_data.get("description", ""),
                    primary_url=primary_url,
                    mirror_urls=mirror_urls,
                )
            )

        return cls(
            model_id=data.get("model_id", ""),
            revision=data.get("revision", ""),
            display_name=data.get("display_name", ""),
            total_size_bytes=data.get("total_size_bytes", 0),
            files=files,
            source_url=data.get("source_url", ""),
        )


# === Exceptions ===


class ModelCacheError(Exception):
    """Base exception for model cache errors."""

    def __init__(self, message: str, code: str = "E_MODEL"):
        self.message = message
        self.code = code
        super().__init__(message)


class DiskFullError(ModelCacheError):
    """Raised when there's insufficient disk space."""

    def __init__(self, required: int, available: int, message: str = ""):
        self.required = required
        self.available = available
        super().__init__(
            message or f"Need {format_bytes(required)}, only {format_bytes(available)} available",
            "E_DISK_FULL",
        )


class NetworkError(ModelCacheError):
    """Raised when network download fails."""

    def __init__(self, message: str, url: str = ""):
        self.url = url
        super().__init__(message, "E_NETWORK")


class CacheCorruptError(ModelCacheError):
    """Raised when cache verification fails."""

    def __init__(self, message: str, file_path: str = ""):
        self.file_path = file_path
        super().__init__(message, "E_CACHE_CORRUPT")


class LockError(ModelCacheError):
    """Raised when unable to acquire cache lock."""

    def __init__(self, message: str = "Unable to acquire cache lock"):
        super().__init__(message, "E_LOCK")


class ModelInUseError(ModelCacheError):
    """Raised when trying to purge a model that's in use."""

    def __init__(self, message: str = "Model is currently in use"):
        super().__init__(message, "E_NOT_READY")


# === Utilities ===


def format_bytes(size: int) -> str:
    """Format byte size for human readability."""
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} PB"


def get_cache_directory() -> Path:
    """Get the platform-specific cache directory."""
    if platform.system() == "Darwin":
        # macOS: ~/Library/Caches/openvoicy
        base = Path.home() / "Library" / "Caches"
    elif platform.system() == "Windows":
        # Windows: %LOCALAPPDATA%\openvoicy\cache
        local_app_data = os.environ.get("LOCALAPPDATA")
        if local_app_data:
            base = Path(local_app_data) / "openvoicy"
        else:
            base = Path.home() / ".cache" / "openvoicy"
    else:
        # Linux/other: ~/.cache/openvoicy
        xdg_cache = os.environ.get("XDG_CACHE_HOME")
        if xdg_cache:
            base = Path(xdg_cache) / "openvoicy"
        else:
            base = Path.home() / ".cache" / "openvoicy"

    return base / "models"


def get_lock_file_path() -> Path:
    """Get the path to the cache lock file."""
    return get_cache_directory() / ".lock"


def compute_sha256(file_path: Path) -> str:
    """Compute SHA-256 hash of a file."""
    sha256 = hashlib.sha256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(DOWNLOAD_CHUNK_SIZE), b""):
            sha256.update(chunk)
    return sha256.hexdigest()


# === Cache Lock ===


class CacheLock:
    """File-based lock for cache operations.

    Uses fcntl on Unix and msvcrt on Windows.
    """

    def __init__(self, timeout: float = LOCK_TIMEOUT_SECONDS):
        self.lock_path = get_lock_file_path()
        self.timeout = timeout
        self._lock_file = None

    def acquire(self) -> bool:
        """Acquire the lock. Returns True if acquired, False if timeout."""
        self.lock_path.parent.mkdir(parents=True, exist_ok=True)

        start_time = time.time()
        while True:
            try:
                self._lock_file = open(self.lock_path, "w")

                if platform.system() == "Windows":
                    import msvcrt

                    msvcrt.locking(self._lock_file.fileno(), msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl

                    fcntl.flock(self._lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

                return True

            except (OSError, IOError):
                if self._lock_file:
                    self._lock_file.close()
                    self._lock_file = None

                if time.time() - start_time > self.timeout:
                    return False

                time.sleep(0.5)

    def release(self) -> None:
        """Release the lock."""
        if self._lock_file:
            try:
                if platform.system() == "Windows":
                    import msvcrt

                    msvcrt.locking(self._lock_file.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    import fcntl

                    fcntl.flock(self._lock_file.fileno(), fcntl.LOCK_UN)
            except (OSError, IOError):
                pass
            finally:
                self._lock_file.close()
                self._lock_file = None

    def __enter__(self):
        if not self.acquire():
            raise LockError("Timeout waiting for cache lock")
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.release()
        return False


# === Disk Space Check ===


def check_disk_space(required_bytes: int) -> None:
    """Check if there's enough disk space.

    Raises:
        DiskFullError: If insufficient space.
    """
    cache_dir = get_cache_directory()
    cache_dir.mkdir(parents=True, exist_ok=True)

    total, used, free = shutil.disk_usage(cache_dir)
    needed = int(required_bytes * DISK_SPACE_BUFFER)

    if free < needed:
        raise DiskFullError(needed, free)


# === Download Functions ===


def download_file(
    url: str,
    dest_path: Path,
    expected_size: int = 0,
    progress_callback: Optional[Callable[[int, int], None]] = None,
) -> None:
    """Download a file with resume support.

    Args:
        url: URL to download from.
        dest_path: Destination file path.
        expected_size: Expected file size (for progress).
        progress_callback: Called with (current, total) bytes.

    Raises:
        NetworkError: On download failure.
    """
    try:
        import urllib.request
        import urllib.error

        # Check for existing partial download
        existing_size = dest_path.stat().st_size if dest_path.exists() else 0

        headers = build_download_headers(existing_size, url)

        request = urllib.request.Request(url, headers=headers)

        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                # Check if server supports range requests
                if existing_size > 0 and response.status == 200:
                    # Server doesn't support Range, restart download
                    existing_size = 0
                    mode = "wb"
                elif response.status == 206:
                    # Partial content, resume
                    mode = "ab"
                elif response.status == 200:
                    mode = "wb"
                else:
                    raise NetworkError(f"Unexpected HTTP status: {response.status}", url)

                # Get content length
                content_length = response.headers.get("Content-Length")
                if content_length:
                    total = existing_size + int(content_length)
                else:
                    total = expected_size if expected_size > 0 else 0

                # Download
                with open(dest_path, mode) as f:
                    downloaded = existing_size
                    while True:
                        chunk = response.read(DOWNLOAD_CHUNK_SIZE)
                        if not chunk:
                            break
                        f.write(chunk)
                        downloaded += len(chunk)
                        if progress_callback:
                            progress_callback(downloaded, total)

        except urllib.error.HTTPError as e:
            raise NetworkError(f"HTTP error {e.code}: {e.reason}", url)
        except urllib.error.URLError as e:
            raise NetworkError(f"URL error: {e.reason}", url)

    except ImportError:
        # Fallback if urllib not available (shouldn't happen)
        raise NetworkError("urllib not available", url)


def is_trusted_hf_download_url(url: str) -> bool:
    """Return True when URL is a trusted Hugging Face HTTPS endpoint."""
    parsed = urlparse(url)
    host = (parsed.hostname or "").lower().rstrip(".")

    if parsed.scheme != "https" or not host:
        return False

    return any(host == trusted or host.endswith(f".{trusted}") for trusted in TRUSTED_HF_HOSTS)


def build_download_headers(existing_size: int = 0, url: str = "") -> dict[str, str]:
    """Build download request headers.

    HuggingFace auth token is sourced from HF_TOKEN env var only.
    It is never persisted in config.
    """
    headers: dict[str, str] = {}
    if existing_size > 0:
        headers["Range"] = f"bytes={existing_size}-"

    hf_token = os.environ.get("HF_TOKEN", "").strip()
    if hf_token and is_trusted_hf_download_url(url):
        headers["Authorization"] = f"Bearer {hf_token}"

    return headers


def download_with_mirrors(
    file_info: ModelFileInfo,
    dest_path: Path,
    progress_callback: Optional[Callable[[int, int], None]] = None,
) -> None:
    """Download a file, trying mirrors on failure.

    Args:
        file_info: File information with URLs.
        dest_path: Destination file path.
        progress_callback: Called with (current, total) bytes.

    Raises:
        NetworkError: If all mirrors fail.
    """
    urls = [file_info.primary_url] + file_info.mirror_urls
    urls = [u for u in urls if u]  # Filter empty URLs

    if not urls:
        raise NetworkError("No download URLs available", "")

    last_error = None

    for url in urls:
        try:
            log(f"Downloading from {url}")
            download_file(url, dest_path, file_info.size_bytes, progress_callback)
            return
        except NetworkError as e:
            last_error = e
            log(f"Download failed from {url}: {e.message}, trying next mirror")

    raise last_error or NetworkError("All mirrors failed", "")


# === Verification ===


def verify_file(file_path: Path, expected_sha256: str, expected_size: int) -> bool:
    """Verify a downloaded file.

    Args:
        file_path: Path to the file.
        expected_sha256: Expected SHA-256 hash (or "VERIFY_ON_FIRST_DOWNLOAD").
        expected_size: Expected file size.

    Returns:
        True if valid, False otherwise.
    """
    if not file_path.exists():
        return False

    # Check size
    actual_size = file_path.stat().st_size
    if expected_size > 0 and actual_size != expected_size:
        log(f"Size mismatch for {file_path}: expected {expected_size}, got {actual_size}")
        return False

    # Check hash (skip if placeholder)
    if expected_sha256 and expected_sha256 != "VERIFY_ON_FIRST_DOWNLOAD":
        actual_sha256 = compute_sha256(file_path)
        if actual_sha256 != expected_sha256:
            log(f"Hash mismatch for {file_path}: expected {expected_sha256}, got {actual_sha256}")
            return False

    return True


def verify_manifest(manifest: ModelManifest, cache_dir: Path) -> bool:
    """Verify all files in a manifest.

    Args:
        manifest: Model manifest.
        cache_dir: Cache directory containing files.

    Returns:
        True if all files are valid.
    """
    for file_info in manifest.files:
        file_path = cache_dir / file_info.path
        if not verify_file(file_path, file_info.sha256, file_info.size_bytes):
            return False
    return True


# === Model Cache Manager ===


class ModelCacheManager:
    """Manages model cache operations."""

    def __init__(self):
        self._state_lock = threading.RLock()
        self._status = ModelStatus.MISSING
        self._progress = DownloadProgress()
        self._error: Optional[str] = None
        self._manifest: Optional[ModelManifest] = None
        self._model_in_use = False

    @property
    def status(self) -> ModelStatus:
        """Get current status."""
        with self._state_lock:
            return self._status

    @property
    def progress(self) -> DownloadProgress:
        """Get current progress."""
        with self._state_lock:
            return DownloadProgress(
                current_bytes=self._progress.current_bytes,
                total_bytes=self._progress.total_bytes,
                current_file=self._progress.current_file,
                files_completed=self._progress.files_completed,
                files_total=self._progress.files_total,
            )

    @property
    def error(self) -> Optional[str]:
        """Get error message if status is ERROR."""
        with self._state_lock:
            return self._error

    def set_model_in_use(self, in_use: bool) -> None:
        """Set whether the model is currently in use."""
        with self._state_lock:
            self._model_in_use = in_use

    def load_manifest(self, manifest_path: Path) -> ModelManifest:
        """Load manifest from file."""
        with open(manifest_path) as f:
            data = json.load(f)
        manifest = ModelManifest.from_dict(data)
        with self._state_lock:
            self._manifest = manifest
        return manifest

    def get_status(self, manifest: Optional[ModelManifest] = None) -> dict[str, Any]:
        """Get current model status.

        Args:
            manifest: Optional manifest (uses loaded manifest if not provided).

        Returns:
            Status dictionary.
        """
        with self._state_lock:
            manifest = manifest or self._manifest
            status = self._status
            error = self._error
            progress = DownloadProgress(
                current_bytes=self._progress.current_bytes,
                total_bytes=self._progress.total_bytes,
                current_file=self._progress.current_file,
                files_completed=self._progress.files_completed,
                files_total=self._progress.files_total,
            )

        cache_dir = get_cache_directory()
        model_dir = cache_dir / manifest.model_id if manifest else cache_dir

        result: dict[str, Any] = {
            "model_id": manifest.model_id if manifest else "unknown",
            "revision": manifest.revision if manifest else "unknown",
            "status": status.value,
            "cache_path": str(model_dir) if manifest else None,
        }

        if status == ModelStatus.DOWNLOADING:
            result["progress"] = progress.to_dict()

        if status == ModelStatus.ERROR and error:
            result["error"] = error

        return result

    def check_cache(
        self,
        manifest: ModelManifest,
        *,
        lock_timeout: float = CHECK_CACHE_LOCK_TIMEOUT_SECONDS,
    ) -> bool:
        """Check if model is already cached and valid.

        Args:
            manifest: Model manifest.

        Returns:
            True if cache is valid.
        """
        try:
            with CacheLock(timeout=lock_timeout):
                return self._check_cache_unlocked(manifest)
        except LockError:
            log("Could not acquire cache lock for check_cache; skipping verification")
            return False

    def _check_cache_unlocked(self, manifest: ModelManifest) -> bool:
        """Check cache state while caller owns the cache lock."""
        cache_dir = get_cache_directory() / manifest.model_id

        if not cache_dir.exists():
            with self._state_lock:
                self._status = ModelStatus.MISSING
            return False

        # Check manifest exists
        cached_manifest = cache_dir / "manifest.json"
        if not cached_manifest.exists():
            with self._state_lock:
                self._status = ModelStatus.MISSING
            return False

        # Verify all files
        with self._state_lock:
            self._status = ModelStatus.VERIFYING
        if verify_manifest(manifest, cache_dir):
            with self._state_lock:
                self._status = ModelStatus.READY
            return True

        with self._state_lock:
            self._status = ModelStatus.MISSING
        return False

    def get_model_path(self, manifest: ModelManifest) -> Path:
        """Get the path to a cached model.

        Args:
            manifest: Model manifest.

        Returns:
            Path to model directory.

        Note:
            Does not verify the cache is valid - use check_cache first.
        """
        return get_cache_directory() / manifest.model_id

    def download_model(
        self,
        manifest: ModelManifest,
        progress_callback: Optional[Callable[[DownloadProgress], None]] = None,
    ) -> Path:
        """Download model files with atomic commit.

        Args:
            manifest: Model manifest.
            progress_callback: Called with progress updates.

        Returns:
            Path to the model directory.

        Raises:
            DiskFullError: Insufficient disk space.
            NetworkError: Download failure.
            CacheCorruptError: Verification failure.
            LockError: Unable to acquire lock.
        """
        with self._state_lock:
            self._manifest = manifest
            self._error = None

        cache_dir = get_cache_directory()
        model_dir = cache_dir / manifest.model_id
        partial_root = cache_dir / ".partial"
        temp_dir = partial_root / manifest.model_id

        try:
            # Check disk space
            check_disk_space(manifest.total_size_bytes)

            # Acquire lock
            with CacheLock():
                # Check if already downloaded
                if self._check_cache_unlocked(manifest):
                    log(f"Model {manifest.model_id} already cached")
                    return model_dir

                # Keep .partial staging across retries so interrupted downloads can resume.
                temp_dir.parent.mkdir(parents=True, exist_ok=True)
                temp_dir.mkdir(parents=True, exist_ok=True)

                # Save manifest to temp
                manifest_path = temp_dir / "manifest.json"
                with open(manifest_path, "w") as f:
                    json.dump(
                        {
                            "model_id": manifest.model_id,
                            "revision": manifest.revision,
                            "display_name": manifest.display_name,
                            "total_size_bytes": manifest.total_size_bytes,
                            "files": [
                                {
                                    "path": fi.path,
                                    "size_bytes": fi.size_bytes,
                                    "sha256": fi.sha256,
                                }
                                for fi in manifest.files
                            ],
                        },
                        f,
                        indent=2,
                    )

                # Download files
                with self._state_lock:
                    self._status = ModelStatus.DOWNLOADING
                    self._progress = DownloadProgress(
                        total_bytes=manifest.total_size_bytes,
                        files_total=len(manifest.files),
                    )

                for i, file_info in enumerate(manifest.files):
                    with self._state_lock:
                        self._progress.current_file = file_info.path
                        self._progress.files_completed = i

                    if progress_callback:
                        progress_callback(self.progress)

                    dest_path = temp_dir / file_info.path
                    dest_path.parent.mkdir(parents=True, exist_ok=True)

                    # Reuse any staged file that already passed integrity checks.
                    if verify_file(dest_path, file_info.sha256, file_info.size_bytes):
                        completed_bytes = sum(fi.size_bytes for fi in manifest.files[: i + 1])
                        with self._state_lock:
                            self._progress.current_bytes = completed_bytes
                        if progress_callback:
                            progress_callback(self.progress)
                        continue

                    # If staged data is larger than expected, restart this file.
                    if (
                        file_info.size_bytes > 0
                        and dest_path.exists()
                        and dest_path.stat().st_size > file_info.size_bytes
                    ):
                        dest_path.unlink()

                    def update_progress(current: int, total: int) -> None:
                        # Calculate total progress across all files
                        completed_bytes = sum(
                            fi.size_bytes for fi in manifest.files[:i]
                        )
                        with self._state_lock:
                            self._progress.current_bytes = completed_bytes + current
                        if progress_callback:
                            progress_callback(self.progress)

                    download_with_mirrors(file_info, dest_path, update_progress)

                    # Verify file
                    with self._state_lock:
                        self._status = ModelStatus.VERIFYING
                    if not verify_file(dest_path, file_info.sha256, file_info.size_bytes):
                        try:
                            dest_path.unlink()
                        except OSError:
                            pass
                        raise CacheCorruptError(
                            f"Verification failed for {file_info.path}", str(dest_path)
                        )

                with self._state_lock:
                    self._progress.files_completed = len(manifest.files)

                # Atomic rename
                if model_dir.exists():
                    shutil.rmtree(model_dir)
                model_dir.parent.mkdir(parents=True, exist_ok=True)
                temp_dir.rename(model_dir)
                temp_dir = None  # Don't clean up on success

                with self._state_lock:
                    self._status = ModelStatus.READY
                log(f"Model {manifest.model_id} downloaded successfully")
                return model_dir

        except (DiskFullError, NetworkError, CacheCorruptError, LockError) as e:
            with self._state_lock:
                self._status = ModelStatus.ERROR
                self._error = str(e)
            raise
        except Exception as e:
            with self._state_lock:
                self._status = ModelStatus.ERROR
                self._error = str(e)
            raise ModelCacheError(str(e))

    def purge_cache(self, model_id: Optional[str] = None) -> bool:
        """Purge model cache.

        Args:
            model_id: Specific model to purge, or None for all.

        Returns:
            True if cache was purged.

        Raises:
            ModelInUseError: If model is currently in use.
        """
        with self._state_lock:
            if self._model_in_use:
                raise ModelInUseError()

        cache_dir = get_cache_directory()

        with CacheLock():
            if model_id:
                model_dir = cache_dir / model_id
                if model_dir.exists():
                    shutil.rmtree(model_dir)
                    log(f"Purged cache for {model_id}")
            else:
                # Purge all models
                if cache_dir.exists():
                    for item in cache_dir.iterdir():
                        if item.is_dir() and not item.name.startswith("."):
                            shutil.rmtree(item)
                    log("Purged all model caches")

        with self._state_lock:
            self._status = ModelStatus.MISSING
        return True


# === Global Instance ===

_manager: Optional[ModelCacheManager] = None


def get_cache_manager() -> ModelCacheManager:
    """Get the global cache manager instance."""
    global _manager
    if _manager is None:
        _manager = ModelCacheManager()
    return _manager


_install_lock = threading.Lock()
_install_thread: Optional[threading.Thread] = None
_install_model_id: Optional[str] = None


def _normalize_model_id(model_id: str) -> str:
    return model_id.strip().lower().replace("\\", "/")


def _model_id_variants(model_id: str) -> set[str]:
    normalized = _normalize_model_id(model_id)
    suffix = normalized.split("/")[-1]
    variants = {normalized, suffix}
    if not normalized.startswith("nvidia/"):
        variants.add(f"nvidia/{suffix}")
    return variants


def _resolve_manifest_path_for_model(model_id: str) -> Path:
    """Resolve model manifest path using catalog/manifests fallbacks."""
    requested_variants = _model_id_variants(model_id)

    catalog_path = resolve_shared_path_optional(MODEL_CATALOG_REL)
    if catalog_path is not None:
        with open(catalog_path) as f:
            catalog = json.load(f)

        for model in catalog.get("models", []):
            catalog_model_id = str(model.get("model_id", "")).strip()
            if _normalize_model_id(catalog_model_id) not in requested_variants:
                continue

            manifest_path = str(model.get("manifest_path", "")).strip()
            if manifest_path:
                relative = (
                    manifest_path
                    if manifest_path.startswith("model/")
                    else f"model/{manifest_path}"
                )
                candidate = resolve_shared_path_optional(relative)
                if candidate is not None:
                    return candidate

    model_slug = _normalize_model_id(model_id).split("/")[-1]
    manifests_candidate = resolve_shared_path_optional(
        f"{MODEL_MANIFESTS_DIR_REL}/{model_slug}.json"
    )
    if manifests_candidate is not None:
        return manifests_candidate

    return resolve_shared_path(MODEL_MANIFEST_REL)


def _emit_model_progress(model_id: str, progress: DownloadProgress) -> None:
    payload = {
        "model_id": model_id,
        "current": progress.current_bytes,
        "total": progress.total_bytes if progress.total_bytes > 0 else None,
        "unit": "bytes",
        "stage": "download",
        "current_file": progress.current_file,
        "files_completed": progress.files_completed,
        "files_total": progress.files_total,
    }
    write_notification(Notification(method="event.model_progress", params=payload))

    # Mirror through status_changed for host integrations that only consume this channel.
    write_notification(
        Notification(
            method="event.status_changed",
            params={
                "state": "loading_model",
                "detail": "Downloading model...",
                "progress": {
                    "current": payload["current"],
                    "total": payload["total"],
                    "unit": payload["unit"],
                    "stage": payload["stage"],
                },
                "model": {
                    "model_id": model_id,
                    "status": "downloading",
                },
            },
        )
    )


def _emit_model_status(model_id: str, status: str, error: Optional[str] = None) -> None:
    payload: dict[str, Any] = {
        "model_id": model_id,
        "status": status,
    }
    if error:
        payload["error"] = error

    write_notification(Notification(method="event.model_status", params=payload))
    write_notification(
        Notification(
            method="event.status_changed",
            params={
                "state": "idle" if status == "ready" else "error",
                "detail": "Model ready" if status == "ready" else error or "Model install failed",
                "model": payload,
            },
        )
    )


def _run_model_install(manager: ModelCacheManager, manifest: ModelManifest) -> None:
    global _install_thread, _install_model_id
    try:
        manager.download_model(
            manifest,
            progress_callback=lambda p: _emit_model_progress(manifest.model_id, p),
        )
        _emit_model_status(manifest.model_id, "ready")
    except (DiskFullError, NetworkError, CacheCorruptError, LockError, ModelCacheError) as e:
        _emit_model_status(manifest.model_id, "error", str(e))
    except Exception as e:
        _emit_model_status(manifest.model_id, "error", str(e))
    finally:
        with _install_lock:
            _install_thread = None
            _install_model_id = None


# === JSON-RPC Handlers ===


def handle_model_get_status(request: Request) -> dict[str, Any]:
    """Handle model.get_status request.

    Returns current model status.
    """
    manager = get_cache_manager()

    # Try to load manifest if not already loaded
    manifest_path = resolve_shared_path_optional(MODEL_MANIFEST_REL)
    if manifest_path is not None:
        try:
            manifest = manager.load_manifest(manifest_path)
            manager.check_cache(manifest)
        except Exception as e:
            log(f"Error loading manifest: {e}")

    return manager.get_status()


def handle_model_download(request: Request) -> dict[str, Any]:
    """Handle model.download request.

    Starts model download.

    Returns:
        status: Current status after starting download.
    """
    manager = get_cache_manager()

    # Load manifest
    try:
        manifest_path = resolve_shared_path(MODEL_MANIFEST_REL)
    except FileNotFoundError:
        raise ModelCacheError("Model manifest not found")

    manifest = manager.load_manifest(manifest_path)

    # Start download (blocking in this implementation)
    # In production, this would be async
    try:
        manager.download_model(manifest)
    except (DiskFullError, NetworkError, CacheCorruptError) as e:
        raise e

    return manager.get_status()


def handle_model_install(request: Request) -> dict[str, Any]:
    """Handle model.install request.

    Starts model install in the background and returns immediately.
    """
    global _install_thread, _install_model_id

    model_id = request.params.get("model_id")
    if not isinstance(model_id, str) or not model_id.strip():
        raise ModelCacheError("model_id is required", "E_INVALID_PARAMS")

    manager = get_cache_manager()
    manifest_path = _resolve_manifest_path_for_model(model_id)
    manifest = manager.load_manifest(manifest_path)

    with _install_lock:
        if _install_thread is not None and _install_thread.is_alive():
            return {
                "model_id": _install_model_id or manifest.model_id,
                "revision": manifest.revision,
                "status": "installing",
                "progress": manager.progress.to_dict(),
            }

        _install_model_id = manifest.model_id
        _install_thread = threading.Thread(
            target=_run_model_install,
            args=(manager, manifest),
            daemon=True,
        )
        _install_thread.start()

    return {
        "model_id": manifest.model_id,
        "revision": manifest.revision,
        "status": "installing",
    }


def handle_model_purge_cache(request: Request) -> dict[str, Any]:
    """Handle model.purge_cache request.

    Purges model cache.

    Params:
        model_id: Optional model ID to purge.

    Returns:
        purged: True if successful.
    """
    model_id = request.params.get("model_id")

    manager = get_cache_manager()

    try:
        manager.purge_cache(model_id)
        return {"purged": True}
    except ModelInUseError:
        raise
