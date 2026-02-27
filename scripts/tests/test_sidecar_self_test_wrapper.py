import json
import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SELF_TEST_WRAPPER = REPO_ROOT / "sidecar" / "self-test"
MOCK_SIDECAR = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json
    import sys


    def result_for(method: str) -> dict:
        if method == "system.ping":
            return {"version": "0.0.0-test", "protocol": "v1"}
        if method == "system.info":
            return {
                "capabilities": ["asr", "replacements", "meter"],
                "runtime": {
                    "python_version": "3.13.0",
                    "platform": "linux",
                    "cuda_available": False,
                },
            }
        if method == "status.get":
            return {
                "state": "idle",
                "model": {"model_id": "test-model", "status": "ready"},
            }
        if method == "replacements.get_rules":
            return {"rules": []}
        if method == "system.shutdown":
            return {"ok": True}
        return {}


    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        request = json.loads(line)
        method = str(request.get("method", ""))
        response = {
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": result_for(method),
        }
        sys.stdout.write(json.dumps(response) + "\\n")
        sys.stdout.flush()
        if method == "system.shutdown":
            break
    """
)


class SidecarSelfTestWrapperTests(unittest.TestCase):
    def test_wrapper_script_exists_and_is_executable(self) -> None:
        """Regression (221f): self-test must be runnable from source tree."""
        self.assertTrue(
            SELF_TEST_WRAPPER.is_file(),
            "sidecar/self-test wrapper script must exist",
        )
        mode = SELF_TEST_WRAPPER.stat().st_mode
        self.assertTrue(
            mode & stat.S_IXUSR,
            "sidecar/self-test must be executable",
        )

    def test_wrapper_sets_pythonpath_to_src(self) -> None:
        content = SELF_TEST_WRAPPER.read_text()
        self.assertIn("PYTHONPATH", content)
        self.assertIn("src", content)
        self.assertIn("openvoicy_sidecar.self_test", content)

    def test_wrapper_executes_self_test_module_with_src_pythonpath(self) -> None:
        """Regression (to6c): execute wrapper path, not just static script assertions."""
        root = Path(tempfile.mkdtemp(prefix="self-test-wrapper-runtime-"))
        try:
            (root / "sidecar" / "src" / "openvoicy_sidecar").mkdir(parents=True, exist_ok=True)
            shutil.copy2(SELF_TEST_WRAPPER, root / "sidecar" / "self-test")

            (root / "sidecar" / "src" / "openvoicy_sidecar" / "__init__.py").write_text(
                "",
                encoding="utf-8",
            )
            (root / "sidecar" / "src" / "openvoicy_sidecar" / "self_test.py").write_text(
                textwrap.dedent(
                    """\
                    import json
                    import os
                    import sys

                    print(
                        json.dumps(
                            {
                                "argv": sys.argv[1:],
                                "pythonpath": os.environ.get("PYTHONPATH", ""),
                            }
                        ),
                        flush=True,
                    )
                    """
                ),
                encoding="utf-8",
            )

            env = os.environ.copy()
            env["PYTHONPATH"] = "/tmp/existing-pythonpath"
            completed = subprocess.run(
                [str(root / "sidecar" / "self-test"), "--probe", "wrapper"],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )

            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )

            payload = json.loads(completed.stdout.strip().splitlines()[-1])
            self.assertEqual(payload["argv"], ["--probe", "wrapper"])

            expected_src = str((root / "sidecar" / "src").resolve())
            pythonpath_parts = [part for part in payload["pythonpath"].split(os.pathsep) if part]
            self.assertGreaterEqual(len(pythonpath_parts), 1)
            self.assertEqual(
                str(Path(pythonpath_parts[0]).resolve()),
                expected_src,
                "wrapper must prepend sidecar/src to PYTHONPATH",
            )
            self.assertIn("/tmp/existing-pythonpath", pythonpath_parts)
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_wrapper_executes_real_self_test_with_sidecar_command_override(self) -> None:
        """Regression (to6c): execute wrapper + self_test subprocess command path end-to-end."""
        root = Path(tempfile.mkdtemp(prefix="self-test-wrapper-command-path-"))
        try:
            shared_root = root / "shared"
            (shared_root / "replacements").mkdir(parents=True, exist_ok=True)
            (shared_root / "model" / "manifests").mkdir(parents=True, exist_ok=True)
            (shared_root / "contracts").mkdir(parents=True, exist_ok=True)
            (shared_root / "replacements" / "PRESETS.json").write_text("{}", encoding="utf-8")
            (shared_root / "model" / "MODEL_MANIFEST.json").write_text(
                '{"model_id":"test-model"}', encoding="utf-8"
            )
            (shared_root / "model" / "MODEL_CATALOG.json").write_text(
                '{"models":[]}', encoding="utf-8"
            )

            mock_sidecar = root / "mock-sidecar.py"
            mock_sidecar.write_text(MOCK_SIDECAR, encoding="utf-8")
            mock_sidecar.chmod(mock_sidecar.stat().st_mode | stat.S_IXUSR)

            env = os.environ.copy()
            env["OPENVOICY_SHARED_ROOT"] = str(shared_root)
            env["OPENVOICY_SIDECAR_COMMAND"] = str(mock_sidecar)

            completed = subprocess.run(
                [str(SELF_TEST_WRAPPER)],
                cwd=REPO_ROOT,
                env=env,
                capture_output=True,
                text=True,
                timeout=45,
                check=False,
            )

            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )
            output = completed.stdout + completed.stderr
            self.assertIn("[SELF_TEST] Starting sidecar process:", output)
            self.assertIn(str(mock_sidecar), output)
            self.assertIn("[SELF_TEST] PASS: All checks passed", output)
        finally:
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
