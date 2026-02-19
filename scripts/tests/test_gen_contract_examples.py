import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "gen_contract_examples.py"
SPEC = importlib.util.spec_from_file_location("gen_contract_examples", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class GenerateContractExamplesTests(unittest.TestCase):
    def test_generate_creates_derived_copy_from_canonical(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            canonical = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            canonical.parent.mkdir(parents=True, exist_ok=True)
            canonical.write_text('{"type":"request","data":{"method":"status.get"}}\n', encoding="utf-8")

            result = MODULE.generate(root)
            self.assertEqual(result, 0)

            derived = root / "shared" / "contracts" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            self.assertTrue(derived.exists())
            self.assertEqual(derived.read_text(encoding="utf-8"), canonical.read_text(encoding="utf-8"))

    def test_check_skips_when_derived_directory_absent(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            canonical = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            canonical.parent.mkdir(parents=True, exist_ok=True)
            canonical.write_text('{"type":"request","data":{"method":"status.get"}}\n', encoding="utf-8")

            result = MODULE.check(root)
            self.assertEqual(result, 0)

    def test_check_fails_when_derived_fixture_drifted(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            canonical = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            canonical.parent.mkdir(parents=True, exist_ok=True)
            canonical.write_text('{"type":"request","data":{"method":"status.get"}}\n', encoding="utf-8")

            derived = root / "shared" / "contracts" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            derived.parent.mkdir(parents=True, exist_ok=True)
            derived.write_text('{"type":"request","data":{"method":"status.get_typo"}}\n', encoding="utf-8")

            result = MODULE.check(root)
            self.assertEqual(result, 1)


if __name__ == "__main__":
    unittest.main()
