"""Isolated tests for the image installer; no network or real models required."""
import contextlib
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("image_setup", Path(__file__).with_name("image-setup.py"))
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class Response:
    def __init__(self, data, length):
        self.data = io.BytesIO(data)
        self.headers = {"Content-Length": str(length)}

    def __enter__(self):
        return self

    def __exit__(self, *args):
        pass

    def read(self, size):
        return self.data.read(size)


class InstallerTests(unittest.TestCase):
    def test_reuses_complete_previous_models_and_preserves_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            previous = Path(tmp) / "old"
            profile = Path(tmp) / "profile"
            name = setup.PADDLE_MODELS[0]
            paddle = previous / "paddlex/official_models" / name
            paddle.mkdir(parents=True)
            for filename in setup.PADDLE_FILES:
                (paddle / filename).write_bytes(filename.encode())
            manga = previous / "manga-model"
            manga.mkdir()
            (manga / "good").write_bytes(b"valid")
            (manga / "bad").write_bytes(b"corrupt")
            models = {"good": hashlib.sha256(b"valid").hexdigest(), "bad": "0" * 64}
            with patch.object(setup, "MANGA_FILES", models):
                setup.reuse_ocr_models(previous, profile)
            copied = profile / "translate-runtime"
            for filename in setup.PADDLE_FILES:
                self.assertEqual((copied / "paddlex/official_models" / name / filename).read_bytes(),
                                 (paddle / filename).read_bytes())
            self.assertEqual((copied / "manga-model/good").read_bytes(), b"valid")
            self.assertFalse((copied / "manga-model/bad").exists())
            self.assertEqual((manga / "bad").read_bytes(), b"corrupt")

    def test_translate_retry_uses_checkout_models_with_existing_managed_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            previous, profile, job = (base / name for name in ("previous", "profile", "job"))
            managed = profile / "translate-runtime"
            venv = managed / "venv"
            python = venv / "Scripts/python.exe"
            python.parent.mkdir(parents=True)
            python.touch()
            (venv / "katran-setup").touch()
            (venv / "katran-ready").touch()
            job.mkdir()
            for index, name in enumerate(setup.PADDLE_MODELS):
                root = managed if index == 0 else previous
                model = root / "paddlex/official_models" / name
                model.mkdir(parents=True)
                for filename in setup.PADDLE_FILES:
                    (model / filename).write_bytes(filename.encode())
            (previous / "manga-model").mkdir()
            (previous / "manga-model/config.json").write_bytes(b"old manga")
            models = base / "ollama-models"
            manifest = models / setup.OLLAMA_MANIFEST
            manifest.parent.mkdir(parents=True)
            manifest.touch()

            def fake_worker(*args, **kwargs):
                if any(str(arg).endswith("manga_recognize.py") for arg in args):
                    args[-1].write_text("[{}]", encoding="utf-8")

            config = {"ocr_root": str(managed), "ocr_reuse_root": str(previous),
                      "paddle_python": str(python), "vision_python": str(python),
                      "ollama": str(base / "ollama.exe"), "ollama_models": str(models)}
            digest = hashlib.sha256(b"old manga").hexdigest()
            with patch.dict(setup.os.environ, {"KATRAN_TRANSLATE_ROOT": ""}), \
                    patch.object(setup, "MANGA_FILES", {"config.json": digest}), \
                    patch.object(setup.subprocess, "run", return_value=SimpleNamespace(returncode=0)) as probe, \
                    patch.object(setup, "run", side_effect=fake_worker) as worker, \
                    patch.object(setup, "sample_image"), \
                    patch.object(setup, "local_ollama", return_value=contextlib.nullcontext((1, {}))), \
                    patch.object(setup, "urlopen", side_effect=AssertionError("network used")):
                setup.translate(config, profile, job, base, install=True)
            self.assertEqual(probe.call_count, 2)
            self.assertTrue(all("-m" not in call.args for call in worker.call_args_list))
            for name in setup.PADDLE_MODELS:
                for filename in setup.PADDLE_FILES:
                    self.assertEqual((managed / "paddlex/official_models" / name / filename).read_bytes(),
                                     filename.encode())
            self.assertEqual((managed / "manga-model/config.json").read_bytes(), b"old manga")
            self.assertEqual((previous / "manga-model/config.json").read_bytes(), b"old manga")

    def test_existing_wrong_digest_is_preserved_without_network(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "model"
            target.write_bytes(b"existing")
            with patch.object(setup, "urlopen", side_effect=AssertionError("network used")):
                with self.assertRaisesRegex(ValueError, "incorrect SHA-256"):
                    setup.download("https://example.invalid", target, 100, "0" * 64)
            self.assertEqual(target.read_bytes(), b"existing")

    def test_short_response_retries_from_start_and_reports_progress(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "model"
            output = io.StringIO()
            digest = hashlib.sha256(b"complete").hexdigest()
            with patch.object(setup, "urlopen", side_effect=[Response(b"bad", 8), Response(b"complete", 8)]) as opened, \
                    patch.object(setup.time, "sleep"), contextlib.redirect_stdout(output):
                setup.download("https://example.invalid", target, 100, digest)
            self.assertEqual(opened.call_count, 2)
            self.assertEqual(target.read_bytes(), b"complete")
            self.assertIn("attempt 2/3", output.getvalue())
            self.assertIn("Retrying from the beginning", output.getvalue())
            self.assertIn("MiB", output.getvalue())


if __name__ == "__main__":
    unittest.main()
