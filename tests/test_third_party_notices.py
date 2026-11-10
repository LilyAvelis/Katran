import importlib.util
import json
import sys
from pathlib import Path
import tempfile
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("notices", Path(__file__).resolve().parents[1] / "scripts/third-party-notices.py")
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


class NoticeContract(unittest.TestCase):
    def test_verbatim_copyright_without_machine_paths(self):
        with tempfile.TemporaryDirectory(prefix="katran-notices-") as temp:
            directory = Path(temp)
            original = "Copyright (c) Synthetic Author\nPermission text exactly retained.\n"
            (directory / "LICENSE-MIT").write_text(original)
            (directory / "lib.rs").write_text("// SPDX-FileCopyrightText: Synthetic Header Owner\nfn do_not_export_source() {}\n")
            metadata = {"packages": [{"id": "synthetic", "name": "synthetic", "version": "1.2.3", "manifest_path": str(directory / "Cargo.toml"), "license": "MIT"}]}
            result = notices.collect(metadata)
            self.assertIn(original, result)
            self.assertNotIn(temp, result)
            self.assertIn("synthetic-1.2.3.crate", result)
            self.assertIn("Synthetic Header Owner", result)
            self.assertNotIn("do_not_export_source", result)

    def test_missing_notice_blocks_distribution(self):
        with tempfile.TemporaryDirectory(prefix="katran-notices-") as temp:
            metadata = {"packages": [{"id": "synthetic", "name": "synthetic", "version": "1.2.3", "manifest_path": str(Path(temp) / "Cargo.toml"), "license": "MIT"}]}
            with self.assertRaisesRegex(ValueError, "No cached license"):
                notices.collect(metadata)

    def test_changed_override_blocks_distribution(self):
        with tempfile.TemporaryDirectory(prefix="katran-notices-") as temp:
            directory = Path(temp)
            (directory / "index.json").write_text(json.dumps({"synthetic@1.2.3": [{"path": "text.txt", "sha256": "0" * 64, "url": "https://example.invalid/license"}]}))
            (directory / "text.txt").write_text("Copyright Synthetic")
            metadata = {"packages": [{"id": "synthetic", "name": "synthetic", "version": "1.2.3", "manifest_path": str(directory / "Cargo.toml"), "license": "MIT"}]}
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                notices.collect(metadata, directory)


if __name__ == "__main__":
    unittest.main()
