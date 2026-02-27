import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SOURCE_SCRIPT = REPO_ROOT / "scripts" / "bundle-sidecar.sh"
TARGET = "x86_64-unknown-linux-gnu"


MOCK_SIDECAR = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json
    import sys

    _ = sys.stdin.read()
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": 1, "result": {"protocol": "v1"}}) + "\\n")
    sys.stdout.flush()
    """
)


class BundleSidecarRuntimeTests(unittest.TestCase):
    def _build_fixture_project(self, include_contracts: bool = True) -> Path:
        root = Path(tempfile.mkdtemp(prefix="bundle-sidecar-runtime-"))
        (root / "scripts").mkdir(parents=True, exist_ok=True)
        (root / "sidecar" / "dist").mkdir(parents=True, exist_ok=True)
        (root / "shared" / "model" / "manifests").mkdir(parents=True, exist_ok=True)
        (root / "shared" / "replacements").mkdir(parents=True, exist_ok=True)
        if include_contracts:
            (root / "shared" / "contracts").mkdir(parents=True, exist_ok=True)

        shutil.copy2(SOURCE_SCRIPT, root / "scripts" / "bundle-sidecar.sh")

        sidecar_bin = root / "sidecar" / "dist" / "openvoicy-sidecar"
        sidecar_bin.write_text(MOCK_SIDECAR, encoding="utf-8")
        sidecar_bin.chmod(sidecar_bin.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        (root / "shared" / "model" / "MODEL_CATALOG.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "model" / "MODEL_MANIFEST.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "model" / "manifests" / "fixture.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "replacements" / "PRESETS.json").write_text("{}", encoding="utf-8")

        if include_contracts:
            (root / "shared" / "contracts" / "tauri.events.v1.json").write_text(
                "{}",
                encoding="utf-8",
            )

        return root

    def _run_script(self, root: Path) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        script = root / "scripts" / "bundle-sidecar.sh"
        return subprocess.run(
            ["bash", str(script), "--target", TARGET],
            cwd=root,
            env=env,
            capture_output=True,
            text=True,
            timeout=90,
            check=False,
        )

    def test_runtime_pass_path_copies_shared_resources_into_tauri_bundle_layout(self) -> None:
        root = self._build_fixture_project(include_contracts=True)
        try:
            completed = self._run_script(root)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )

            tauri_shared = root / "src-tauri" / "binaries" / "shared"
            self.assertTrue((tauri_shared / "model" / "MODEL_CATALOG.json").is_file())
            self.assertTrue((tauri_shared / "model" / "MODEL_MANIFEST.json").is_file())
            self.assertTrue((tauri_shared / "model" / "manifests" / "fixture.json").is_file())
            self.assertTrue((tauri_shared / "contracts" / "tauri.events.v1.json").is_file())
            self.assertTrue((tauri_shared / "replacements" / "PRESETS.json").is_file())
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_failure_path_exits_nonzero_when_contracts_source_missing(self) -> None:
        root = self._build_fixture_project(include_contracts=False)
        try:
            completed = self._run_script(root)
            self.assertEqual(
                completed.returncode,
                1,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            self.assertIn("Contracts directory not found", completed.stdout + completed.stderr)
        finally:
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
