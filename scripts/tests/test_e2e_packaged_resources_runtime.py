import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SOURCE_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-packaged-resources.sh"


MOCK_SIDECAR = textwrap.dedent(
    """\
#!/usr/bin/env python3
import json
import os
import sys


def response_payload() -> dict:
        if os.environ.get("MOCK_BAD_SYSTEM_INFO") == "1":
            return {"version": "mock"}
        if os.environ.get("MOCK_LEGACY_SYSTEM_INFO") == "1":
            return {
                "capabilities": {
                    "cuda_available": False,
                    "supports_progress": True,
                },
                "runtime": {
                    "python": "3.13.0",
                    "platform": "linux",
                },
                "resource_paths": {},
            }

        shared_root = os.environ["OPENVOICY_SHARED_ROOT"]
        return {
            "capabilities": ["asr", "replacements", "meter"],
            "runtime": {
                "python_version": "3.13.0",
                "platform": "linux",
                "cuda_available": False,
            },
            "resource_paths": {
                "shared_root": shared_root,
                "presets": os.path.join(shared_root, "replacements", "PRESETS.json"),
                "model_manifest": os.path.join(shared_root, "model", "MODEL_MANIFEST.json"),
                "model_catalog": os.path.join(shared_root, "model", "MODEL_CATALOG.json"),
                "contracts_dir": os.path.join(shared_root, "contracts"),
            }
        }


raw = sys.stdin.read().strip()
req = {"id": 1}
if raw:
    req = json.loads(raw.splitlines()[0])

out = {
    "jsonrpc": "2.0",
    "id": req.get("id", 1),
    "result": response_payload(),
}
sys.stdout.write(json.dumps(out) + "\\n")
sys.stdout.flush()
"""
)

MOCK_SELF_TEST = textwrap.dedent(
    """\
    import os
    import sys
    from pathlib import Path


    if os.environ.get("MOCK_SELF_TEST_FAIL") == "1":
        raise SystemExit(1)

    sidecar_cmd = os.environ.get("OPENVOICY_SIDECAR_COMMAND", "")
    shared_root = os.environ.get("OPENVOICY_SHARED_ROOT", "")
    if not sidecar_cmd or not Path(sidecar_cmd).is_file():
        raise SystemExit("missing OPENVOICY_SIDECAR_COMMAND")
    if not shared_root or not Path(shared_root).is_dir():
        raise SystemExit("missing OPENVOICY_SHARED_ROOT")

    print("mock packaged self_test: OK", flush=True)
    """
)


class PackagedResourcesRuntimeTests(unittest.TestCase):
    target = "x86_64-unknown-linux-gnu"
    macos_target = "x86_64-apple-darwin"
    windows_target = "x86_64-pc-windows-msvc"

    def _build_fixture_project(self, target: str) -> Path:
        root = Path(tempfile.mkdtemp(prefix="packaged-resources-runtime-"))
        (root / "scripts" / "e2e").mkdir(parents=True, exist_ok=True)
        (root / "src-tauri" / "binaries").mkdir(parents=True, exist_ok=True)
        (root / "sidecar" / "src" / "openvoicy_sidecar").mkdir(parents=True, exist_ok=True)
        (root / "shared" / "replacements").mkdir(parents=True, exist_ok=True)
        (root / "shared" / "model" / "manifests").mkdir(parents=True, exist_ok=True)
        (root / "shared" / "contracts").mkdir(parents=True, exist_ok=True)

        shutil.copy2(SOURCE_SCRIPT, root / "scripts" / "e2e" / "test-packaged-resources.sh")

        sidecar_name = f"openvoicy-sidecar-{target}"
        if "windows" in target:
            sidecar_name = f"{sidecar_name}.exe"
        sidecar_bin = root / "src-tauri" / "binaries" / sidecar_name
        sidecar_bin.write_text(MOCK_SIDECAR, encoding="utf-8")
        sidecar_bin.chmod(sidecar_bin.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        (root / "shared" / "replacements" / "PRESETS.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "model" / "MODEL_MANIFEST.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "model" / "MODEL_CATALOG.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "model" / "manifests" / "fixture.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "contracts" / "tauri.events.v1.json").write_text("{}", encoding="utf-8")
        (root / "sidecar" / "src" / "openvoicy_sidecar" / "__init__.py").write_text(
            "", encoding="utf-8"
        )
        (root / "sidecar" / "src" / "openvoicy_sidecar" / "self_test.py").write_text(
            MOCK_SELF_TEST,
            encoding="utf-8",
        )

        return root

    def _run_script(
        self,
        root: Path,
        target: str,
        extra_env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        sidecar_src = str(root / "sidecar" / "src")
        if env.get("PYTHONPATH"):
            env["PYTHONPATH"] = sidecar_src + os.pathsep + env["PYTHONPATH"]
        else:
            env["PYTHONPATH"] = sidecar_src
        if extra_env:
            env.update(extra_env)

        script = root / "scripts" / "e2e" / "test-packaged-resources.sh"
        return subprocess.run(
            ["bash", str(script), "--target", target],
            cwd=root,
            env=env,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )

    def test_runtime_pass_path_exits_zero_and_validates_resource_paths(self) -> None:
        root = self._build_fixture_project(self.target)
        try:
            completed = self._run_script(root, self.target)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            self.assertIn(
                "system.info resource path validation: OK",
                completed.stdout + completed.stderr,
            )
            self.assertIn("system.info schema preflight: OK", completed.stdout + completed.stderr)
            self.assertIn("packaged sidecar self-test passed", completed.stdout + completed.stderr)
            self.assertIn("mock packaged self_test: OK", completed.stdout + completed.stderr)
            self.assertIn("packaged resource smoke test passed", completed.stdout + completed.stderr)
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_invalid_system_info_payload_exits_nonzero(self) -> None:
        root = self._build_fixture_project(self.target)
        try:
            completed = self._run_script(
                root,
                self.target,
                extra_env={"MOCK_BAD_SYSTEM_INFO": "1"},
            )
            self.assertEqual(
                completed.returncode,
                1,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            self.assertIn(
                "system.info schema preflight failed",
                completed.stdout + completed.stderr,
            )
            self.assertIn(
                "result.capabilities must be string[]",
                completed.stdout + completed.stderr,
            )
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_legacy_system_info_schema_exits_nonzero(self) -> None:
        root = self._build_fixture_project(self.target)
        try:
            completed = self._run_script(
                root,
                self.target,
                extra_env={"MOCK_LEGACY_SYSTEM_INFO": "1"},
            )
            self.assertEqual(
                completed.returncode,
                1,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            self.assertIn(
                "bundled sidecar appears stale",
                completed.stdout + completed.stderr,
            )
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_self_test_failure_exits_nonzero(self) -> None:
        root = self._build_fixture_project(self.target)
        try:
            completed = self._run_script(
                root,
                self.target,
                extra_env={"MOCK_SELF_TEST_FAIL": "1"},
            )
            self.assertEqual(
                completed.returncode,
                1,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_macos_target_passes(self) -> None:
        root = self._build_fixture_project(self.macos_target)
        try:
            completed = self._run_script(root, self.macos_target)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            self.assertIn("packaged resource smoke test passed", completed.stdout + completed.stderr)
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_windows_target_passes_with_exe_binary(self) -> None:
        root = self._build_fixture_project(self.windows_target)
        try:
            completed = self._run_script(root, self.windows_target)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            self.assertIn("packaged resource smoke test passed", completed.stdout + completed.stderr)
        finally:
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
