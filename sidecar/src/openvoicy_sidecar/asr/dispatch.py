"""Backend dispatch: maps model family to ASR backend class.

The registry enables dynamic backend selection based on the model catalog
family field, so adding a new backend only requires registering it here.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from ..protocol import log
from .base import ASRError

if TYPE_CHECKING:
    from .base import ASRBackend

# family -> callable that returns an ASRBackend instance
_REGISTRY: dict[str, type] = {}


class UnsupportedFamilyError(ASRError):
    """Raised when model family has no registered backend."""

    code = "E_UNSUPPORTED_FAMILY"


def register_backend(family: str, cls: type) -> None:
    """Register a backend class for a model family."""
    key = family.strip().lower()
    _REGISTRY[key] = cls
    log(f"Registered ASR backend: family={key} class={cls.__name__}")


def get_backend(family: str) -> ASRBackend:
    """Instantiate and return the backend for the given model family.

    Raises:
        UnsupportedFamilyError: If no backend is registered for the family.
    """
    key = family.strip().lower()
    cls = _REGISTRY.get(key)
    if cls is None:
        known = ", ".join(sorted(_REGISTRY)) or "(none)"
        raise UnsupportedFamilyError(
            f"No backend registered for model family '{family}'. Known families: {known}"
        )
    log(f"Selected ASR backend: family={key} class={cls.__name__}")
    return cls()


def registered_families() -> list[str]:
    """Return sorted list of registered family names."""
    return sorted(_REGISTRY)


# Auto-register built-in backends
def _auto_register() -> None:
    from .parakeet import ParakeetBackend

    register_backend("parakeet", ParakeetBackend)

    try:
        from .whisper import WhisperBackend

        register_backend("whisper", WhisperBackend)
    except ImportError as error:
        log(f"Whisper backend import unavailable: {error}")


_auto_register()
