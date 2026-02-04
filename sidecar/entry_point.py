#!/usr/bin/env python3
"""PyInstaller entry point for OpenVoicy Sidecar.

This wrapper avoids relative import issues when running as a frozen binary.
"""

import sys
import os

# Ensure the package is importable
if getattr(sys, 'frozen', False):
    # Running as frozen binary
    bundle_dir = sys._MEIPASS
    sys.path.insert(0, bundle_dir)
else:
    # Running as script - add src to path
    src_dir = os.path.join(os.path.dirname(__file__), 'src')
    sys.path.insert(0, src_dir)

from openvoicy_sidecar.server import run_server

if __name__ == "__main__":
    run_server()
