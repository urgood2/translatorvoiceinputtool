import unittest
from pathlib import Path


IPC_COMPLIANCE_PATH = Path("sidecar/tests/test_ipc_compliance.py")


class SidecarIpcComplianceReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.content = IPC_COMPLIANCE_PATH.read_text(encoding="utf-8")

    def test_shutdown_orphan_check_not_linux_only(self) -> None:
        self.assertNotIn("requires Linux /proc support", self.content)
        self.assertIn("observed_descendants.update(_list_descendant_pids(proc.pid))", self.content)

    def test_cross_platform_descendant_helpers_exist(self) -> None:
        self.assertIn("def _list_descendant_pids_posix(", self.content)
        self.assertIn("def _list_descendant_pids_windows(", self.content)
        self.assertIn("def _list_descendant_pids(root_pid: int) -> set[int]:", self.content)
        self.assertIn("def _pid_exists(pid: int) -> bool:", self.content)


if __name__ == "__main__":
    unittest.main()
