import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_brownfield_compatibility.py"
SPEC = importlib.util.spec_from_file_location("check_brownfield_compatibility", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


def _reference_doc_text() -> str:
    rows = "\n".join(
        [
            "| Module | Impact | Notes |",
            "|---|---|---|",
            "| `src-tauri/src/state.rs` | No semantic changes | Keep state enum stable. |",
            "| `src-tauri/src/config.rs` | Additive fields only | Extend config safely. |",
            "| `src-tauri/src/history.rs` | Extended entry | Additive metadata. |",
            "| `src-tauri/src/integration.rs` | Orchestrator role preserved | Keep session gating. |",
            "| `src-tauri/src/commands.rs` | Remove TODOs, add new commands | Existing signatures stable. |",
            "| `src-tauri/src/watchdog.rs` | Evolved into supervisor | No rewrite. |",
            "| `src-tauri/src/injection.rs` | Minor updates | Preserve flow. |",
            "| `src-tauri/src/tray.rs` | Dynamic menu builder | Extend behavior. |",
            "| `src/hooks/useTauriEvents.ts` | Listen to canonical events only | Legacy aliases retired; keep canonical listener set in sync with `tauri.events.v1.json`. |",
            "| `src/types.ts` | Extended with new types | Backward compatible. |",
            "| `shared/ipc/IPC_PROTOCOL_V1.md` | Additive only | IPC v1 locked. |",
            "| `shared/schema/AppConfig.schema.json` | Additive fields only | Explicit additions only. |",
            "| `sidecar/` | Bug fixes plus new methods | Additive behavior. |",
        ]
    )

    rules = "\n".join(MODULE.REQUIRED_RULE_PREFIXES)

    return (
        "# Brownfield Compatibility Reference\n\n"
        "Derived from planning/PLAN.md Appendix A.\n\n"
        "## Module Impact Map\n\n"
        f"{rows}\n\n"
        "## Critical Implementation Rules\n\n"
        f"{rules}\n"
    )


class BrownfieldCompatibilityGuardTests(unittest.TestCase):
    @staticmethod
    def _write_required_tree(root: Path, doc_text: str) -> None:
        (root / "shared").mkdir(parents=True)
        (root / "shared" / "ipc").mkdir(parents=True)
        (root / "shared" / "schema").mkdir(parents=True)
        (root / "src-tauri" / "src").mkdir(parents=True)
        (root / "src" / "hooks").mkdir(parents=True)
        (root / "sidecar").mkdir(parents=True)

        (root / "shared" / "BROWNFIELD_COMPATIBILITY.md").write_text(doc_text)
        (root / "shared" / "ipc" / "IPC_PROTOCOL_V1.md").write_text("# IPC")
        (root / "shared" / "schema" / "AppConfig.schema.json").write_text("{}")

        for path in [
            "src-tauri/src/state.rs",
            "src-tauri/src/config.rs",
            "src-tauri/src/history.rs",
            "src-tauri/src/integration.rs",
            "src-tauri/src/commands.rs",
            "src-tauri/src/watchdog.rs",
            "src-tauri/src/injection.rs",
            "src-tauri/src/tray.rs",
            "src/hooks/useTauriEvents.ts",
            "src/types.ts",
        ]:
            full = root / path
            full.parent.mkdir(parents=True, exist_ok=True)
            full.write_text("// stub\n")

    def test_guard_passes_for_valid_reference_and_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            self._write_required_tree(root, _reference_doc_text())
            errors = MODULE.validate_brownfield_compatibility(root)
            self.assertEqual(errors, [])

    def test_guard_fails_when_required_mapping_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            broken_doc = _reference_doc_text().replace("`src-tauri/src/history.rs`", "`src-tauri/src/history_missing.rs`")
            self._write_required_tree(root, broken_doc)
            errors = MODULE.validate_brownfield_compatibility(root)
            self.assertTrue(
                any("Missing required module mapping" in error for error in errors),
                errors,
            )

    def test_guard_fails_when_required_path_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            self._write_required_tree(root, _reference_doc_text())
            (root / "src-tauri" / "src" / "integration.rs").unlink()
            errors = MODULE.validate_brownfield_compatibility(root)
            self.assertTrue(
                any("Required mapped file does not exist: src-tauri/src/integration.rs" in error for error in errors),
                errors,
            )


if __name__ == "__main__":
    unittest.main()
