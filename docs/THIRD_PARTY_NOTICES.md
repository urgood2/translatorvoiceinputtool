# Third-Party Notices

This file contains the licenses and notices for third-party software included in or distributed with OpenVoicy.

---

## ASR Model

### NVIDIA Parakeet TDT 0.6B v3
- **Source**: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
- **License**: CC-BY-4.0 (Creative Commons Attribution 4.0 International)
- **Copyright**: NVIDIA Corporation
- **Notes**: The model is downloaded on first run and cached locally. Attribution to NVIDIA is required when distributing or using the model.

---

## Rust Dependencies

The following Rust crates are used in the Tauri backend:

### tauri
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/tauri
- **Copyright**: The Tauri Programme within The Commons Conservancy

### tauri-plugin-shell
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/plugins-workspace

### serde
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/serde-rs/serde
- **Copyright**: David Tolnay

### serde_json
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/serde-rs/json

### tokio
- **Version**: 1.x
- **License**: MIT
- **Source**: https://github.com/tokio-rs/tokio
- **Copyright**: Tokio Contributors

### thiserror
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/dtolnay/thiserror

### log
- **Version**: 0.4.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/rust-lang/log

### env_logger
- **Version**: 0.11.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/rust-cli/env_logger

### chrono
- **Version**: 0.4.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/chronotope/chrono

### uuid
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/uuid-rs/uuid

### once_cell
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/matklad/once_cell

### regex
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/rust-lang/regex

### phf
- **Version**: 0.11.x
- **License**: MIT
- **Source**: https://github.com/sfackler/rust-phf

### dirs
- **Version**: 5.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/dirs-dev/dirs-rs

### global-hotkey
- **Version**: 0.6.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/global-hotkey

### png
- **Version**: 0.17.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/image-rs/image-png

---

## Python Dependencies

The following Python packages are used in the sidecar:

### sounddevice
- **Version**: >=0.4.6
- **License**: MIT
- **Source**: https://github.com/spatialaudio/python-sounddevice
- **Copyright**: Matthias Geier
- **Notes**: Provides Python bindings for PortAudio

### numpy
- **Version**: >=1.24.0
- **License**: BSD-3-Clause
- **Source**: https://github.com/numpy/numpy
- **Copyright**: NumPy Developers

### scipy
- **Version**: >=1.10.0
- **License**: BSD-3-Clause
- **Source**: https://github.com/scipy/scipy
- **Copyright**: SciPy Developers

### PyTorch (torch)
- **Version**: >=2.0.0
- **License**: BSD-3-Clause
- **Source**: https://github.com/pytorch/pytorch
- **Copyright**: Meta Platforms, Inc. and affiliates
- **Notes**: Runtime dependency for ASR inference. Users install separately based on their hardware (CPU/CUDA). PyTorch includes modified components from various open source projects, see full NOTICE at https://github.com/pytorch/pytorch/blob/main/NOTICE

### NVIDIA NeMo Toolkit (nemo_toolkit)
- **Version**: >=2.0.0
- **License**: Apache-2.0
- **Source**: https://github.com/NVIDIA/NeMo
- **Copyright**: NVIDIA Corporation
- **Notes**: Runtime dependency for loading and running Parakeet ASR models. Install with `pip install nemo_toolkit[asr]` for ASR functionality.

---

## JavaScript Dependencies

The following JavaScript packages are used in the React frontend:

### React
- **Version**: 18.x
- **License**: MIT
- **Source**: https://github.com/facebook/react
- **Copyright**: Meta Platforms, Inc. and affiliates

### React DOM
- **Version**: 18.x
- **License**: MIT
- **Source**: https://github.com/facebook/react

### Zustand
- **Version**: 5.x
- **License**: MIT
- **Source**: https://github.com/pmndrs/zustand
- **Copyright**: Daishi Kato

### @tauri-apps/api
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/tauri

### @tauri-apps/plugin-shell
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/plugins-workspace

---

## Audio Libraries

### PortAudio
- **Version**: 19.x (bundled via sounddevice)
- **License**: MIT
- **Source**: http://www.portaudio.com/
- **Copyright**: Ross Bencina, Phil Burk
- **Notes**: Cross-platform audio I/O library, bundled in the sidecar binary

---

## Icons and Audio Assets

The following assets are original works created for this project and are licensed under the same terms as the project itself:

### Tray Icons
- `tray-idle-*.png` - Idle state indicator (microphone icon)
- `tray-recording-*.png` - Recording state indicator (red microphone)
- `tray-transcribing-*.png` - Transcribing state indicator (processing)
- `tray-loading-*.png` - Loading/initializing state
- `tray-error-*.png` - Error state indicator
- `tray-disabled-*.png` - Disabled state indicator

### Audio Cues
- `cue-start.wav` - Recording start notification sound
- `cue-stop.wav` - Recording stop notification sound
- `cue-error.wav` - Error notification sound

These assets were programmatically generated or created specifically for OpenVoicy and do not require third-party attribution.

---

## Build and Development Tools

The following tools are used during development and are not redistributed:

- **Vite** - MIT License
- **TypeScript** - Apache-2.0 License
- **Tailwind CSS** - MIT License
- **ESLint** - MIT License
- **Vitest** - MIT License
- **Hatch** (Python) - MIT License
- **Ruff** (Python) - MIT License

---

## License Texts

### MIT License

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Apache License 2.0

```
Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

### BSD 3-Clause License

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### Creative Commons Attribution 4.0 International (CC-BY-4.0)

The NVIDIA Parakeet model is licensed under CC-BY-4.0. This license allows:
- Sharing and adapting the material
- Commercial use

With the requirement to:
- Give appropriate credit to NVIDIA
- Indicate if changes were made
- Not apply additional restrictions

Full license text: https://creativecommons.org/licenses/by/4.0/legalcode

---

## Compliance Notes

1. **No GPL Dependencies**: This project does not include any GPL or AGPL licensed dependencies to ensure compatibility with the overall project license.

2. **Model Attribution**: The NVIDIA Parakeet model requires attribution. This is satisfied by this notice file and the model information displayed in the application settings.

3. **Transitive Dependencies**: Transitive dependencies inherit compatible licenses (MIT, Apache-2.0, BSD, MPL-2.0). No incompatible licenses are introduced through transitive dependencies.

4. **MPL-2.0 Transitive Dependencies**: Some CSS-related Rust crates (`cssparser`, `selectors`, `dtoa-short`) use MPL-2.0 licensing. MPL-2.0 is file-based copyleft and compatible with MIT/Apache-2.0 when the MPL-licensed files are not modified.

5. **Unicode License**: ICU-related crates use the Unicode-3.0 license, which is a permissive license allowing redistribution with or without modification.

---

*Last updated: 2026-02-05*
