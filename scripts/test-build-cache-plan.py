"""Synthetic storage-policy checks; no application/profile files are accessed."""
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("cache_plan", Path(__file__).with_name("build-cache-plan.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CachePlanTests(unittest.TestCase):
    def test_budget_preserves_dependency_libraries_and_release_outputs(self):
        with tempfile.TemporaryDirectory() as fixture:
            root = Path(fixture)

            def write(name, size):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"x" * size)
                return path

            duplicate = write("debug/old.exe", 80)
            exe = write("build/debug/deps/fixture-0123456789abcdef.exe", 70)
            pdb = write("build/debug/deps/fixture-0123456789abcdef.pdb", 30)
            library = write("build/debug/deps/libdependency.rlib", 30)
            release = write("build/release/katran-ui.exe", 20)
            write("build/debug/incremental/old/session.bin", 20)
            write("build/debug/incremental/new/session.bin", 10)
            os.utime(root / "build/debug/incremental/old", (1, 1))
            os.utime(root / "build/debug/incremental/new", (2, 2))
            result = module.plan(root, 70 / 1024**3, 10 / 1024**3)
            removed = {entry["relative"] for entry in result["items"]}
            self.assertEqual(result["before_bytes"], 260)
            self.assertEqual(result["planned_after_bytes"], 60)
            self.assertIn(str(exe.relative_to(root)), removed)
            self.assertIn(str(pdb.relative_to(root)), removed)
            self.assertNotIn(str(library.relative_to(root)), removed)
            self.assertNotIn(str(release.relative_to(root)), removed)
            self.assertTrue(duplicate.exists(), "planning must be read-only")
            self.assertFalse(module.plan(root, 1, 1)["items"])

    def test_unremovable_dependency_budget_is_reported(self):
        with tempfile.TemporaryDirectory() as fixture:
            root = Path(fixture)
            library = root / "build/debug/deps/dependency.rlib"
            library.parent.mkdir(parents=True)
            library.write_bytes(b"x" * 100)
            result = module.plan(root, 10 / 1024**3)
            self.assertTrue(result["over_budget"])
            self.assertFalse(result["items"])
            self.assertTrue(library.exists())


if __name__ == "__main__":
    unittest.main()
