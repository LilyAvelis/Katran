"""Install or exercise the image workers with synthetic input in a visible terminal."""
import contextlib
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
from urllib.request import ProxyHandler, build_opener, urlopen
from urllib.error import URLError
import zipfile


UPSCALE_MODELS = {
    "4x_foolhardy_Remacri.pth": (
        "https://huggingface.co/JCTN/ESRGAN/resolve/main/4x_foolhardy_Remacri.pth",
        "e1a73bd89c2da1ae494774746398689048b5a892bd9653e146713f9df8bca86a",
        (2, 4),
    ),
    "RealESRGAN_x2plus.pth": (
        "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.1/RealESRGAN_x2plus.pth",
        "49fafd45f8fd7aa8d31ab2a22d14d91b536c34494a5cfe31eb5d89c2fa266abb",
        (2,),
    ),
    "RealESRGAN_x4.pth": (
        "https://huggingface.co/ai-forever/Real-ESRGAN/resolve/main/RealESRGAN_x4.pth",
        "aa00f09ad753d88576b21ed977e97d634976377031b178acc3b5b238df463400",
        (4,),
    ),
}
PADDLE_MODELS = ("PP-OCRv5_server_det", "PP-OCRv5_server_rec", "korean_PP-OCRv5_mobile_rec")
PADDLE_FILES = ("inference.json", "inference.pdiparams", "inference.yml")
MANGA_FILES = {
    "config.json": "8c0e395de8fa699daaac21aee33a4ba9bd1309cfbff03147813d2a025f39f349",
    "preprocessor_config.json": "af4eb4d79cf61b47010fc0bc9352ee967579c417423b4917188d809b7e048948",
    "pytorch_model.bin": "c63e0bb5b3ff798c5991de18a8e0956c7ee6d1563aca6729029815eda6f5c2eb",
    "vocab.txt": "344fbb6b8bf18c57839e924e2c9365434697e0227fac00b88bb4899b78aa594d",
    "special_tokens_map.json": "303df45a03609e4ead04bc3dc1536d0ab19b5358db685b6f3da123d05ec200e3",
    "tokenizer_config.json": "d775ad1deac162dc56b84e9b8638f95ed8a1f263d0f56f4f40834e26e205e266",
}
MANGA_SOURCE = "https://huggingface.co/kha-white/manga-ocr-base/resolve/aa6573bd10b0d446cbf622e29c3e084914df9741/"
OLLAMA_MODEL = "qwen3:4b-instruct"
OLLAMA_MANIFEST = Path("manifests/registry.ollama.ai/library/qwen3/4b-instruct")


def say(message):
    print(message, flush=True)


def ordinary_paths(value):
    if isinstance(value, dict):
        return {key: ordinary_paths(path) for key, path in value.items()}
    if isinstance(value, str) and value.startswith("\\\\?\\"):
        # These paths came from Rust's canonicalized profile on Windows.
        rest = value[4:]
        if rest.lower().startswith("unc\\"):
            return "\\\\" + rest[4:]
        if rest[1:3] == ":\\":
            return rest
    return value


def run(*args, timeout=600, **kwargs):
    subprocess.run(args, check=True, timeout=timeout, **kwargs)


def managed_path(profile, relative):
    path = profile / relative
    if not path.resolve().is_relative_to(profile.resolve()):
        raise ValueError(f"Katran's {relative} directory points outside this profile")
    return path


def download(url, target, limit, digest=None):
    if target.is_file() and target.stat().st_size > 0:
        return
    if target.exists():
        raise ValueError(f"{target.name} already exists but is incomplete. Inspect it before replacing it.")
    target.parent.mkdir(parents=True, exist_ok=True)
    say(f"Downloading {target.name} ...")
    fd, staging = tempfile.mkstemp(prefix=".download-", dir=target.parent)
    try:
        with os.fdopen(fd, "wb") as out:
            for attempt in range(3):
                sha = hashlib.sha256()
                size = 0
                out.seek(0)
                out.truncate()
                try:
                    with urlopen(url, timeout=45) as response:
                        for block in iter(lambda: response.read(1024 * 1024), b""):
                            size += len(block)
                            if size > limit:
                                raise ValueError(f"{target.name} exceeds its download size limit")
                            sha.update(block)
                            out.write(block)
                    break
                except (TimeoutError, URLError, ConnectionError):
                    if attempt == 2:
                        raise
                    say(f"Connection interrupted for {target.name}. Retrying ...")
                    time.sleep(2)
        if not size or (digest and sha.hexdigest() != digest):
            raise ValueError(f"{target.name} download is empty or its SHA-256 is incorrect")
        os.rename(staging, target)
    finally:
        Path(staging).unlink(missing_ok=True)


def environment(path, packages, torch=False, existing=None, probe=None):
    if existing and Path(existing).is_file():
        if not probe or subprocess.run([existing, "-I", "-c", probe], timeout=90).returncode == 0:
            return Path(existing)
        say("The existing image Python failed the import check. Preparing an isolated Katran environment.")
    if path.exists() and not (path / "katran-setup").is_file():
        raise ValueError(f"{path.name} already exists. The installer will not change an unowned Python environment.")
    python = path / "Scripts/python.exe"
    if (path / "katran-ready").is_file() and python.is_file():
        return python
    path.mkdir(parents=True, exist_ok=True)
    (path / "katran-setup").touch()
    if not python.is_file():
        run(sys.executable, "-I", "-m", "venv", path)
    run(python, "-I", "-m", "pip", "--isolated", "install",
        "--index-url", "https://pypi.org/simple", "pip==25.3")
    if torch:
        run(python, "-I", "-m", "pip", "--isolated", "install",
            "--index-url", "https://download.pytorch.org/whl/cpu",
            "--extra-index-url", "https://pypi.org/simple", "--only-binary=:all:",
            "torch==2.10.0+cpu", "torchvision==0.25.0+cpu", timeout=1800)
    run(python, "-I", "-m", "pip", "--isolated", "install",
        "--index-url", "https://pypi.org/simple", *packages, timeout=1800)
    if probe:
        run(python, "-I", "-c", probe, timeout=90)
    (path / "katran-ready").write_text("1", encoding="ascii")
    return python


def sample_image(python, path):
    run(python, "-I", "-c", "from PIL import Image, ImageDraw; import sys; "
        "im = Image.new('RGB', (96, 64), 'white'); "
        "ImageDraw.Draw(im).text((10, 18), 'Katran', fill='black'); im.save(sys.argv[1])", path)


def upscale(config, profile, job, scripts, install):
    python = config["python"]
    models = config["models"]
    if install:
        python = environment(managed_path(profile, "upscale-runtime/venv"),
                             ("pillow==12.1.1", "numpy==2.2.6", "spandrel==0.4.2"),
                             torch=True, existing=python, probe="import torch, PIL, numpy, spandrel")
        for name, (url, digest, _) in UPSCALE_MODELS.items():
            if not models.get(name):
                target = managed_path(profile, "ai-models/upscale_models") / name
                download(url, target, 256 * 1024**2, digest)
                models[name] = target
    missing = [name for name in UPSCALE_MODELS if not models.get(name)]
    if missing:
        raise ValueError(f"Upscale weights missing: {', '.join(missing)}. Use Install.")
    source, output, receipt = (job / name for name in ("input.png", "output.png", "upscale.json"))
    sample_image(python, source)
    for name, (_, _, factors) in UPSCALE_MODELS.items():
        for factor in factors:
            say(f"Checking {name} at {factor}x ...")
            output.unlink(missing_ok=True)
            receipt.unlink(missing_ok=True)
            try:
                run(python, "-E", scripts / "image-upscale/worker.py",
                    source, output, receipt, models[name], str(factor))
            finally:
                if receipt.is_file():
                    say(receipt.read_text(encoding="utf-8"))
            result = json.loads(receipt.read_text(encoding="utf-8"))
            if not output.is_file() or (result.get("width"), result.get("height")) != (96 * factor, 64 * factor):
                raise ValueError(f"{name} produced unexpected output")


def install_paddle_model(model_root, name, job):
    target = model_root / name
    if all((target / file).is_file() and (target / file).stat().st_size for file in PADDLE_FILES):
        return
    if target.exists():
        raise ValueError(f"Existing Paddle model {name} is incomplete and was left intact.")
    archive = job / f"{name}.tar"
    download("https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/"
             f"paddle3.0.0/{name}_infer.tar", archive, 1024**3)
    extracted = job / name
    extracted.mkdir()
    with tarfile.open(archive) as source:
        for file in PADDLE_FILES:
            matches = [member for member in source.getmembers()
                       if member.isfile() and Path(member.name).name == file and 0 < member.size <= 1024**3]
            if len(matches) != 1:
                raise ValueError(f"Missing or ambiguous {file} in Paddle model {name}")
            with source.extractfile(matches[0]) as src, (extracted / file).open("xb") as dst:
                shutil.copyfileobj(src, dst)
    model_root.mkdir(parents=True, exist_ok=True)
    extracted.rename(target)
    archive.unlink()


def install_ollama(profile, job):
    target = managed_path(profile, "translate-ollama-runtime")
    if target.exists():
        raise ValueError("Existing Katran Ollama runtime is incomplete and was left intact.")
    archive = job / "ollama-windows-amd64.zip"
    download("https://github.com/ollama/ollama/releases/download/v0.32.1/ollama-windows-amd64.zip",
             archive, 3 * 1024**3,
             "d5abdc21b64ee928d3c92880ac22da5e5b0a46b8b07179791dd8c711b35f8397")
    extracted = job / "ollama"
    with zipfile.ZipFile(archive) as source:
        source.extractall(extracted)
    if not (extracted / "ollama.exe").is_file():
        raise ValueError("The Ollama ZIP does not contain ollama.exe")
    extracted.rename(target)
    archive.unlink()
    return target / "ollama.exe"


@contextlib.contextmanager
def local_ollama(executable, models):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    env = dict(os.environ, OLLAMA_MODELS=str(models), OLLAMA_HOST=f"127.0.0.1:{port}",
               OLLAMA_NO_CLOUD="1", OLLAMA_NOPRUNE="1", NO_PROXY="127.0.0.1,localhost")
    server = subprocess.Popen([executable, "serve"], env=env, cwd=executable.parent,
                              creationflags=subprocess.CREATE_NEW_PROCESS_GROUP)
    try:
        opener = build_opener(ProxyHandler({}))
        for _ in range(120):
            if server.poll() is not None:
                raise ValueError("Private Ollama server stopped before it became ready")
            try:
                with opener.open(f"http://127.0.0.1:{port}/api/version", timeout=1) as response:
                    if not json.loads(response.read(4096)).get("version") or server.poll() is not None:
                        raise ValueError("Private Ollama server readiness check failed")
                    break
            except OSError:
                time.sleep(0.25)
        else:
            raise TimeoutError("Private Ollama server startup timed out")
        try:
            yield port, env
        finally:
            with contextlib.suppress(OSError, subprocess.SubprocessError):
                run(executable, "stop", OLLAMA_MODEL, env=env, timeout=10)
    finally:
        if server.poll() is None:
            try:
                server.send_signal(signal.CTRL_BREAK_EVENT)
                server.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                server.terminate()
                server.wait(timeout=5)


def translate(config, profile, job, scripts, install):
    os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
    root = Path(config["ocr_root"])
    paddle, vision = config["paddle_python"], config["vision_python"]
    ollama, models = config["ollama"], config["ollama_models"]
    if install:
        if os.getenv("KATRAN_TRANSLATE_ROOT") and root != profile / "translate-runtime":
            say("Using the explicitly configured OCR directory as-is.")
        else:
            managed_root = managed_path(profile, "translate-runtime")
            if managed_root != root:
                paddle = None
            root = managed_root
            paddle = environment(managed_path(profile, "translate-runtime/venv"),
                                 ("paddlepaddle==3.2.2", "paddleocr==3.3.3", "paddlex==3.3.13"),
                                 existing=paddle, probe="import paddle; from paddleocr import PaddleOCR")
            for name in PADDLE_MODELS:
                install_paddle_model(managed_path(profile, "translate-runtime/paddlex/official_models"), name, job)
            for name, digest in MANGA_FILES.items():
                target = managed_path(profile, "translate-runtime/manga-model") / name
                download(MANGA_SOURCE + name + "?download=true", target, 1024**3, digest)
        vision = environment(managed_path(profile, "translate-vision-runtime/venv"),
                             ("pillow==12.1.1", "numpy==2.2.6", "opencv-python==4.13.0.92",
                              "transformers==5.2.0"), torch=True, existing=vision,
                             probe="import cv2; from transformers import ViTImageProcessor, VisionEncoderDecoderModel")
        if not ollama:
            if os.getenv("KATRAN_OLLAMA_EXE"):
                raise ValueError("KATRAN_OLLAMA_EXE does not point to an existing Ollama executable")
            ollama = install_ollama(profile, job)
        models = models or managed_path(profile, "translate-ollama")
    if not all((paddle, vision, ollama, models)):
        raise ValueError("Translation runtime or Qwen is missing. Use Install.")
    ocr_models = root / "paddlex/official_models"
    for name in PADDLE_MODELS:
        if not all((ocr_models / name / file).is_file() for file in PADDLE_FILES):
            raise ValueError(f"Paddle model {name} is incomplete. Use Install or check the configured directory.")
    if not all((root / "manga-model" / file).is_file() for file in MANGA_FILES):
        raise ValueError("Manga OCR files are incomplete. Use Install or check the configured directory.")
    source = job / "input.png"
    sample_image(vision, source)
    env = dict(os.environ, HF_HUB_DISABLE_PROGRESS_BARS="1",
               KATRAN_PADDLE_MODELS=str(ocr_models), KATRAN_PROCESS_CACHE=str(job),
               KATRAN_MANGA_MODEL=str(root / "manga-model"))
    worker = scripts / "image-translate"
    for language in ("japan", "china", "korean"):
        say(f"Checking Paddle OCR ({language}) ...")
        run(paddle, "-E", worker / "ocr_page.py",
            source, job / f"{language}.json", "--lang", language, env=env)
    say("Checking Manga OCR ...")
    regions = job / "regions.json"
    regions.write_text(json.dumps([{"polygon": [[2, 10], [85, 10], [85, 40], [2, 40]],
                                    "text": "", "confidence": 1.0}]), encoding="utf-8")
    manga_result = job / "manga.json"
    run(vision, "-E", worker / "manga_recognize.py", source, regions, manga_result, env=env)
    if not json.loads(manga_result.read_text(encoding="utf-8")):
        raise ValueError("Manga OCR did not process the synthetic region")
    manifest = Path(models) / OLLAMA_MANIFEST
    if not install and not manifest.is_file():
        raise ValueError(f"Ollama model {OLLAMA_MODEL} is missing. Use Install.")
    with local_ollama(Path(ollama), Path(models)) as (port, ollama_env):
        if not manifest.is_file():
            say(f"Downloading {OLLAMA_MODEL} ...")
            run(ollama, "pull", OLLAMA_MODEL, env=ollama_env, timeout=7200)
        say("Checking one translation with full GPU placement ...")
        run(vision, "-I", "-c", "import sys; sys.path.insert(0, sys.argv[1]); "
            "from translate_page import translate; "
            "translate([{'source_text': '\\u3053\\u3093\\u306b\\u3061\\u306f'}], 'ja', 'en', sys.argv[2], int(sys.argv[3]))",
            worker, OLLAMA_MODEL, str(port), timeout=250)


def main():
    request, profile, job, bundle = map(Path, sys.argv[1:])
    config = json.loads(request.read_text(encoding="utf-8"))
    operation = {"upscale": upscale, "translate": translate}[config["component"]]
    operation(ordinary_paths(config["image"]), profile, job, bundle / "scripts", config["action"] == "install")
    say("Functional verification succeeded.")


if __name__ == "__main__":
    main()
