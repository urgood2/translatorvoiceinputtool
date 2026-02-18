import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
REPAIR_SCRIPT = REPO_ROOT / "scripts" / "repair-bd-hooks.sh"


class RepairBdHooksTests(unittest.TestCase):
    def test_hooks_skip_cleanly_when_bd_has_no_hook_commands(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            hooks_dir = repo_root / ".git" / "hooks"
            hooks_dir.mkdir(parents=True)

            fake_bin = repo_root / "bin"
            fake_bin.mkdir()
            fake_bd = fake_bin / "bd"
            fake_bd.write_text(
                "#!/usr/bin/env sh\n"
                "if [ \"$1\" = \"hook\" ] || [ \"$1\" = \"hooks\" ]; then\n"
                "  exit 2\n"
                "fi\n"
                "exit 0\n"
            )
            fake_bd.chmod(0o755)

            env = dict(os.environ)
            env["PATH"] = f"{fake_bin}:{env.get('PATH', '')}"

            subprocess.run(
                [str(REPAIR_SCRIPT)],
                cwd=repo_root,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

            pre_commit_hook = hooks_dir / "pre-commit"
            self.assertTrue(pre_commit_hook.exists())
            self.assertTrue(os.access(pre_commit_hook, os.X_OK))

            result = subprocess.run(
                [str(pre_commit_hook)],
                cwd=repo_root,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0)
            self.assertIn("does not provide hook commands", result.stderr)


if __name__ == "__main__":
    unittest.main()
