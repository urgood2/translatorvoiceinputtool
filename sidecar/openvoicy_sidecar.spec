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

Platform support:
- Linux x64: bundles system PortAudio
- macOS x64/arm64: uses sounddevice's bundled PortAudio
- Windows x64: uses sounddevice's bundled PortAudio
"""

import sys
import os
import platform
from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

block_cipher = None

# Collect sounddevice's bundled PortAudio library
sounddevice_datas = collect_data_files('sounddevice')
sounddevice_binaries = collect_dynamic_libs('sounddevice')

# Platform-specific binary bundling
current_os = platform.system()
current_arch = platform.machine()

if current_os == 'Linux':
    # Linux: bundle system PortAudio if available
    # Check common locations for different distros
    portaudio_paths = [
        '/lib/x86_64-linux-gnu/libportaudio.so.2',      # Debian/Ubuntu x64
        '/usr/lib/x86_64-linux-gnu/libportaudio.so.2',  # Alternative location
        '/lib/aarch64-linux-gnu/libportaudio.so.2',    # Debian/Ubuntu arm64
        '/usr/lib/libportaudio.so.2',                   # Generic
        '/usr/lib64/libportaudio.so.2',                 # RHEL/Fedora
    ]
    for lib_path in portaudio_paths:
        if os.path.exists(lib_path):
            sounddevice_binaries.append((lib_path, '.'))
            break

elif current_os == 'Darwin':
    # macOS: sounddevice bundles PortAudio, but ensure it's collected
    # Also need to handle universal binary considerations
    pass  # collect_dynamic_libs should handle this

elif current_os == 'Windows':
    # Windows: sounddevice bundles PortAudio DLLs
    pass  # collect_dynamic_libs should handle this

# Add the openvoicy_sidecar package
package_datas = [('src/openvoicy_sidecar', 'openvoicy_sidecar')]
shared_datas = [
    ('../shared/contracts', 'shared/contracts'),
    ('../shared/model', 'shared/model'),
    ('../shared/replacements', 'shared/replacements'),
]

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
    datas=sounddevice_datas + package_datas + shared_datas,
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
