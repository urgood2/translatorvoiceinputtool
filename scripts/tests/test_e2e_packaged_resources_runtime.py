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


def system_info_payload() -> dict:
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
        },
    }


def rpc_result(method: str) -> dict:
    if method == "system.info":
        return system_info_payload()
    if method == "system.ping":
        return {"version": "0.0.0-mock", "protocol": "v1"}
    if method == "status.get":
        return {
            "state": "idle",
            "detail": "mock idle",
            "model": {"model_id": "mock-model", "status": "ready"},
        }
    if method == "replacements.get_rules":
        return {"rules": []}
    if method == "system.shutdown":
        return {"accepted": True}
    raise KeyError(method)


for raw_line in sys.stdin:
    raw_line = raw_line.strip()
    if not raw_line:
        continue

    try:
        req = json.loads(raw_line)
    except json.JSONDecodeError:
        continue

    req_id = req.get("id", 1)
    method = req.get("method", "")
    try:
        out = {"jsonrpc": "2.0", "id": req_id, "result": rpc_result(method)}
    except KeyError:
        out = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {"code": -32601, "message": f"method not found: {method}"},
        }

    sys.stdout.write(json.dumps(out) + "\\n")
    sys.stdout.flush()

    if method == "system.shutdown":
        break
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

    def _build_fixture_project(self, target: str, use_real_self_test: bool = False) -> Path:
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
        (root / "shared" / "model" / "MODEL_MANIFEST.json").write_text(
            '{"model_id":"fixture-model"}',
            encoding="utf-8",
        )
        (root / "shared" / "model" / "MODEL_CATALOG.json").write_text(
            '{"models":[{"model_id":"fixture-model"}]}',
            encoding="utf-8",
        )
        (root / "shared" / "model" / "manifests" / "fixture.json").write_text("{}", encoding="utf-8")
        (root / "shared" / "contracts" / "tauri.events.v1.json").write_text("{}", encoding="utf-8")
        (root / "sidecar" / "src" / "openvoicy_sidecar" / "__init__.py").write_text(
            "", encoding="utf-8"
        )
        if use_real_self_test:
            shutil.copy2(
                REPO_ROOT / "sidecar" / "src" / "openvoicy_sidecar" / "self_test.py",
                root / "sidecar" / "src" / "openvoicy_sidecar" / "self_test.py",
            )
            shutil.copy2(
                REPO_ROOT / "sidecar" / "src" / "openvoicy_sidecar" / "resources.py",
                root / "sidecar" / "src" / "openvoicy_sidecar" / "resources.py",
            )
        else:
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

    def test_runtime_pass_path_exits_zero_with_real_self_test_module(self) -> None:
        root = self._build_fixture_project(self.target, use_real_self_test=True)
        try:
            completed = self._run_script(root, self.target)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            output = completed.stdout + completed.stderr
            self.assertIn("system.info schema preflight: OK", output)
            self.assertIn("[SELF_TEST] Testing system.ping... OK", output)
            self.assertIn("[SELF_TEST] PASS: All checks passed", output)
            self.assertIn("packaged sidecar self-test passed", output)
            self.assertIn("packaged resource smoke test passed", output)
            self.assertNotIn("mock packaged self_test: OK", output)
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
