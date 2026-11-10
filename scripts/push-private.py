"""Push the current branch using a local, Git-ignored credential file."""
import base64
import argparse
import os
import re
from pathlib import Path
import subprocess
import sys
from urllib.parse import urlparse

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--tag", help="Push one existing stable release tag without force")
arguments = parser.parse_args()
if arguments.tag and not re.fullmatch(r"v\d+\.\d+\.\d+", arguments.tag):
    parser.error("Expected a stable release tag such as v2.0.0")

root = Path(__file__).resolve().parent.parent
token = os.environ.get("GITHUB_TOKEN", "")
if not token:
    env_file = root / ".env.local"
    if env_file.exists():
        for line in env_file.read_text(encoding="utf-8-sig").splitlines():
            if line.startswith("GITHUB_TOKEN="):
                token = line.split("=", 1)[1].strip().strip('"').strip("'")
if not token:
    sys.exit("GITHUB_TOKEN is missing from the environment or .env.local")

remote = subprocess.check_output(
    ["git", "remote", "get-url", "origin"], cwd=root, text=True
).strip()
url = urlparse(remote)
if url.scheme != "https" or url.hostname != "github.com" or url.username:
    sys.exit("Expected a credential-free GitHub HTTPS origin")

env = {k: v for k, v in os.environ.items()
       if not k.startswith("GIT_TRACE") and k != "GIT_CURL_VERBOSE"}
env.update({
    "GIT_CONFIG_COUNT": "1",
    "GIT_CONFIG_KEY_0": "http.https://github.com/.extraheader",
    "GIT_CONFIG_VALUE_0": "AUTHORIZATION: basic " + base64.b64encode(
        ("x-access-token:" + token).encode()
    ).decode(),
    "GIT_TERMINAL_PROMPT": "0",
})
push = ["git", "-c", "credential.helper=", "push"]
if arguments.tag:
    push += ["origin", "refs/tags/" + arguments.tag]
else:
    push += ["-u", "origin", "HEAD"]
result = subprocess.run(
    push,
    cwd=root, env=env,
)
sys.exit(result.returncode)
