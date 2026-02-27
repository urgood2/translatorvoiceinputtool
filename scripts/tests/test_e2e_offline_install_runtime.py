import os
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SOURCE_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-offline-install.sh"
SOURCE_COMMON = REPO_ROOT / "scripts" / "e2e" / "lib" / "common.sh"
SOURCE_LOG = REPO_ROOT / "scripts" / "e2e" / "lib" / "log.sh"


MOCK_SIDECAR = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json
    import os
    import shutil
    import sys
    import urllib.error
    import urllib.request
    from pathlib import Path


    def cache_root() -> Path:
        base = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache")))
        return base / "openvoicy" / "models"


    def manifest_path() -> Path:
        shared_root = os.environ.get("OPENVOICY_SHARED_ROOT")
        if not shared_root:
            raise RuntimeError("OPENVOICY_SHARED_ROOT is required for mock sidecar")
        return Path(shared_root) / "model" / "MODEL_MANIFEST.json"


    def load_manifest() -> dict:
        with manifest_path().open("r", encoding="utf-8") as f:
            return json.load(f)


    def model_cached(manifest: dict) -> bool:
        root = cache_root() / manifest["model_id"]
        for file_info in manifest.get("files", []):
            if not (root / file_info["path"]).is_file():
                return False
        return (root / "manifest.json").is_file()


    def write_cached_model(manifest: dict, payload: bytes) -> None:
        root = cache_root() / manifest["model_id"]
        root.mkdir(parents=True, exist_ok=True)
        for file_info in manifest.get("files", []):
            target = root / file_info["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(payload)
        (root / "manifest.json").write_text(
            json.dumps(
                {
                    "model_id": manifest["model_id"],
                    "revision": manifest.get("revision", "mock"),
                }
            ),
            encoding="utf-8",
        )


    def success(request_id: int, result: dict) -> None:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}) + "\\n")
        sys.stdout.flush()


    def error(request_id: int, message: str, kind: str) -> None:
        payload = {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32000, "message": message, "data": {"kind": kind}},
        }
        sys.stdout.write(json.dumps(payload) + "\\n")
        sys.stdout.flush()


    def purge(model_id: str | None) -> list[str]:
        root = cache_root()
        if not root.exists():
            return []
        if model_id:
            target = root / model_id
            if target.exists():
                shutil.rmtree(target)
                return [model_id]
            return []
        purged: list[str] = []
        for child in root.iterdir():
            if child.is_dir():
                purged.append(child.name)
                shutil.rmtree(child)
        return purged


    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        request = json.loads(line)
        request_id = request.get("id")
        method = request.get("method", "")
        params = request.get("params") or {}

        if method == "system.ping":
            success(request_id, {"protocol": "v1"})
            continue

        if method == "status.get":
            success(request_id, {"state": "idle", "model": {"status": "unknown"}})
            continue

        if method == "asr.initialize":
            manifest = load_manifest()
            if model_cached(manifest):
                success(request_id, {"status": "ready", "model_id": manifest["model_id"]})
            else:
                error(request_id, "Model not ready", "E_MODEL_NOT_READY")
            continue

        if method == "model.purge_cache":
            purged_ids = purge(params.get("model_id"))
            success(request_id, {"purged": True, "purged_model_ids": purged_ids})
            continue

        if method == "model.download":
            manifest = load_manifest()
            url = manifest["source_url"]
            try:
                with urllib.request.urlopen(url, timeout=2) as response:
                    data = response.read()
                write_cached_model(manifest, data)
                success(request_id, {"status": "ready", "model_id": manifest["model_id"]})
            except (urllib.error.URLError, TimeoutError, OSError):
                kind = os.environ.get("MOCK_DOWNLOAD_ERROR_KIND", "E_NETWORK")
                message = os.environ.get(
                    "MOCK_DOWNLOAD_ERROR_MESSAGE",
                    "Check your internet connection and retry",
                )
                error(request_id, message, kind)
            continue

        if method == "system.shutdown":
            success(request_id, {"ok": True})
            break

        error(request_id, f"unknown method: {method}", "E_INVALID_PARAMS")
    """
)


class OfflineInstallRuntimeTests(unittest.TestCase):
    def _create_temp_project(self, include_default_cache: bool) -> tuple[Path, Path]:
        root = Path(tempfile.mkdtemp(prefix="offline-install-runtime-"))
        (root / "scripts" / "e2e" / "lib").mkdir(parents=True, exist_ok=True)
        (root / "sidecar" / "dist").mkdir(parents=True, exist_ok=True)
        (root / "shared" / "model").mkdir(parents=True, exist_ok=True)

        shutil.copy2(SOURCE_SCRIPT, root / "scripts" / "e2e" / "test-offline-install.sh")
        shutil.copy2(SOURCE_COMMON, root / "scripts" / "e2e" / "lib" / "common.sh")
        shutil.copy2(SOURCE_LOG, root / "scripts" / "e2e" / "lib" / "log.sh")

        sidecar_bin = root / "sidecar" / "dist" / "openvoicy-sidecar"
        sidecar_bin.write_text(MOCK_SIDECAR, encoding="utf-8")
        sidecar_bin.chmod(sidecar_bin.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        (root / "shared" / "model" / "MODEL_MANIFEST.json").write_text(
            """
            {
              "model_id": "default-model",
              "files": [{"path": "default.bin"}]
            }
            """.strip(),
            encoding="utf-8",
        )

        cache_home = root / ".default-cache"
        if include_default_cache:
            model_dir = cache_home / "openvoicy" / "models" / "default-model"
            model_dir.mkdir(parents=True, exist_ok=True)
            (model_dir / "default.bin").write_bytes(b"default-cache")

        return root, cache_home

    def _run_script(
        self,
        root: Path,
        cache_home: Path,
        extra_env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop("OPENVOICY_SHARED_ROOT", None)
        env["HOME"] = str(root / ".home")
        env["XDG_CACHE_HOME"] = str(cache_home)
        env["E2E_TIMEOUT_RUNNER"] = "python3"
        if extra_env:
            env.update(extra_env)

        script = root / "scripts" / "e2e" / "test-offline-install.sh"
        return subprocess.run(
            ["bash", str(script)],
            cwd=root,
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )

    def _latest_log_text(self, root: Path) -> str:
        logs = sorted((root / "logs" / "e2e").glob("test-offline-install-*.log"))
        self.assertTrue(logs, "expected offline-install log file to be created")
        return logs[-1].read_text(encoding="utf-8")

    def test_runtime_pass_path_covers_offline_error_atomicity_retry_and_exit_zero(self) -> None:
        root, cache_home = self._create_temp_project(include_default_cache=True)
        try:
            completed = self._run_script(root, cache_home)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )

            log_text = self._latest_log_text(root)
            self.assertIn("assert_pass model.download returns E_NETWORK when offline", log_text)
            self.assertIn("atomic cache state verified", log_text)
            self.assertIn("assert_pass retry succeeds with network restored", log_text)
            self.assertIn("[INFO] [result] PASS", log_text)
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_failure_path_returns_exit_one_for_non_network_error_kind(self) -> None:
        root, cache_home = self._create_temp_project(include_default_cache=True)
        try:
            completed = self._run_script(
                root,
                cache_home,
                extra_env={"MOCK_DOWNLOAD_ERROR_KIND": "E_TIMEOUT"},
            )
            self.assertEqual(
                completed.returncode,
                1,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )

            log_text = self._latest_log_text(root)
            self.assertIn("assert_fail model.download returns E_NETWORK when offline", log_text)
        finally:
            shutil.rmtree(root, ignore_errors=True)

    def test_runtime_skip_path_returns_77_when_default_cache_missing(self) -> None:
        root, cache_home = self._create_temp_project(include_default_cache=False)
        try:
            completed = self._run_script(root, cache_home)
            self.assertEqual(
                completed.returncode,
                77,
                msg=f"stdout={completed.stdout}\\nstderr={completed.stderr}",
            )

            log_text = self._latest_log_text(root)
            self.assertIn("[WARN] [result] SKIP", log_text)
        finally:
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
