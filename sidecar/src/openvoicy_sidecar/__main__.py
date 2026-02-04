"""Entry point for the OpenVoicy sidecar."""

from .server import run_server


def main() -> None:
    """Main entry point."""
    run_server()


if __name__ == "__main__":
    main()
