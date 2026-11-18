"""Block Git refs containing plausible Notion integration tokens."""

import argparse
import hashlib
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
GIT_PATTERN = r"(ntn_|secret_)[A-Za-z0-9_-]{24,}"
TOKEN_PATTERN = re.compile(GIT_PATTERN.encode("ascii"))
# Exact digests of the two synthetic literals in historical Notion unit tests.
FIXTURE_DIGESTS = {
    "60ffbd711af26c0570af669932ce52a39f085e3e73b1aec96b925a2296113eb1",
    "63a58577e05ca6c43e8252236f5a19a37c4c1dc89c785ba1a3df73c874bb8cdb",
}


def git(*args):
    return subprocess.check_output(["git", "--no-replace-objects", *args], cwd=ROOT)


def contains_secret(data):
    return any(
        hashlib.sha256(match.group()).hexdigest() not in FIXTURE_DIGESTS
        for match in TOKEN_PATTERN.finditer(data)
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", default="HEAD", help="Ref to be pushed")
    parser.add_argument("--all", action="store_true", help="Audit all local refs")
    args = parser.parse_args()
    try:
        if git("rev-parse", "--is-shallow-repository").strip() != b"false":
            print("Complete Git history is required for the Notion credential check.", file=sys.stderr)
            return 2
        git("rev-parse", "--verify", "--end-of-options", args.ref)
        refs = ["--all", "HEAD"] if args.all else [args.ref]
        changes = git(
            "log", "--full-history", "-m", "--format=", "--no-color",
            "--no-ext-diff", "--no-textconv", "--no-renames", "--text",
            "-p", "-U0", "-G", GIT_PATTERN, *refs, "--",
        )
        messages = git("log", "--format=%B", *refs, "--")
        tags = git("for-each-ref", "--format=%(contents)", "refs/tags") if args.all else (
            git("for-each-ref", "--format=%(contents)", args.ref)
            if args.ref.startswith("refs/tags/") else b""
        )
    except subprocess.CalledProcessError:
        print("Notion credential check could not complete. Push blocked.", file=sys.stderr)
        return 2
    if any(contains_secret(data) for data in (changes, messages, tags)):
        print("Possible Notion credential in Git history or metadata. Push blocked.", file=sys.stderr)
        return 1
    print("Notion credential check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
