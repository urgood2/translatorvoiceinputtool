# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec file for OpenVoicy Sidecar.

This builds a single-file executable containing:
- Python runtime
- Audio capture (sounddevice + PortAudio)
- NumPy/SciPy for audio processing
- JSON-RPC server

Build command:
    pyinstaller openvoicy_sidecar.spec

Output: dist/openvoicy-sidecar (or openvoicy-sidecar.exe on Windows)
"""

import sys
from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

block_cipher = None

# Collect sounddevice's bundled PortAudio library
sounddevice_datas = collect_data_files('sounddevice')
sounddevice_binaries = collect_dynamic_libs('sounddevice')

# Add system PortAudio library (Linux)
import platform
if platform.system() == 'Linux':
    portaudio_lib = '/lib/x86_64-linux-gnu/libportaudio.so.2'
    import os
    if os.path.exists(portaudio_lib):
        sounddevice_binaries.append((portaudio_lib, '.'))

# Add the openvoicy_sidecar package
package_datas = [('src/openvoicy_sidecar', 'openvoicy_sidecar')]

# Hidden imports that PyInstaller might miss
hidden_imports = [
    'sounddevice',
    'numpy',
    'scipy',
    'scipy.signal',
    'scipy.fft',
    # These are commonly missed by PyInstaller
    'numpy.core._methods',
    'numpy.lib.format',
]

a = Analysis(
    ['entry_point.py'],
    pathex=[],
    binaries=sounddevice_binaries,
    datas=sounddevice_datas + package_datas,
    hiddenimports=hidden_imports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        # Exclude unnecessary modules to reduce size
        'tkinter',
        'matplotlib',
        'PIL',
        'cv2',
        'IPython',
        'jupyter',
        'notebook',
        'sphinx',
        'docutils',
        'test',
        'tests',
        'unittest',
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name='openvoicy-sidecar',
    debug=False,
    bootloader_ignore_signals=False,
    strip=True,  # Strip symbols to reduce size
    upx=True,    # Use UPX compression if available
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,  # Console app for stdio JSON-RPC
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
