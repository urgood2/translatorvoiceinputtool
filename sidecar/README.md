# OpenVoicy Sidecar

Voice-to-text ASR sidecar for the Voice Input Tool.

## Development

```bash
# Install runtime + test dependencies (includes numpy)
pip install -e ".[test]"

# Alternative with uv
uv sync --extra test

# Run tests
pytest tests/
```
