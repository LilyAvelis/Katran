"""Offline synthetic validation of AI diagnostic boundaries."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("verify_components", Path(__file__).with_name("verify.py"))
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


class VerificationContract(unittest.TestCase):
    def test_missing_model_is_explicit_and_never_uses_a_folder_index(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(verify.ModelMissing):
                verify.model_files(Path(directory))

    def test_existing_incomplete_folder_does_not_fall_back_to_emoji(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "folder-ai-runtime").mkdir()
            (root / "emoji-ai-runtime").mkdir()
            with self.assertRaises(FileNotFoundError):
                verify.model_files(root)

    def test_mismatched_model_digest_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "synthetic"
            file.write_bytes(b"not a model")
            with self.assertRaises(ValueError):
                verify.check_hash(file, "0" * 64)


if __name__ == "__main__":
    unittest.main()
