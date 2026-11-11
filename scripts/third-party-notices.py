"""Collect verbatim notices from Cargo's cached, locked dependency sources.

No network, credentials, machine paths or dependency source code are exported.
An explicit metadata JSON fixture may be used for synthetic verification.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import subprocess


def collect(metadata, overrides_directory=None):
    sections = ["Katran — third-party dependency notices\n"
                "Dependency license expressions are informational. The original license\n"
                "and notice texts below retain their original copyright statements.\n"]
    workspace = set(metadata.get("workspace_members", []))
    override_index = {}
    if overrides_directory:
        override_index = json.loads((overrides_directory / "index.json").read_text(encoding="utf-8"))
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["id"] in workspace:
            continue
        directory = Path(package["manifest_path"]).parent
        files = set()
        if package.get("license_file"):
            files.add(directory / package["license_file"])
        # Include common root notices and their conventional subdirectories.
        for folder in (directory, directory / "licenses", directory / "LICENSES"):
            if folder.is_dir():
                for item in folder.iterdir():
                    name = item.name.upper()
                    if item.is_file() and name.startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT")):
                        files.add(item)
        # Bundled default fonts carry additional mandatory OFL/Ubuntu/MIT texts.
        if package["name"] == "epaint_default_fonts":
            files.update((directory / "fonts").glob("*.txt"))
        overrides = override_index.get(f"{package['name']}@{package['version']}", [])
        if not files and not overrides:
            raise ValueError(f"No cached license text for {package['name']} {package['version']}; review before redistribution")
        sections.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\nLicense: {package.get('license') or 'see license text'}\n")
        sections.append(f"Unmodified upstream source: https://static.crates.io/crates/{package['name']}/{package['name']}-{package['version']}.crate\n")
        # REUSE-style crates sometimes place copyright owners only in source
        # headers, while their LICENSES/MIT.txt contains the generic grant.
        copyright_headers = set()
        for source in directory.rglob("*"):
            if source.is_file() and source.suffix in (".rs", ".c", ".h", ".cpp"):
                with source.open(encoding="utf-8", errors="replace") as stream:
                    for line in itertools.islice(stream, 40):
                        stripped = line.strip()
                        if "copyright" in stripped.lower() and stripped.startswith(("//", "/*", "*", "#")):
                            copyright_headers.add(stripped)
        if copyright_headers:
            sections.append("Original source copyright headers:\n" + "\n".join(sorted(copyright_headers)) + "\n")
        for file in sorted(files, key=lambda p: p.relative_to(directory).as_posix()):
            label = file.relative_to(directory).as_posix()
            sections.append(f"\n--- {label} ---\n" + file.read_text(encoding="utf-8-sig") + "\n")
        for entry in overrides:
            path = overrides_directory / entry["path"]
            if path.parent.resolve() != overrides_directory.resolve():
                raise ValueError("Override must be a direct child of license-texts")
            data = path.read_bytes()
            if hashlib.sha256(data).hexdigest() != entry["sha256"]:
                raise ValueError(f"License override hash mismatch: {entry['path']}")
            sections.append(f"\n--- {entry['path']} ---\nUpstream notice: {entry['url']}\n")
            if entry.get("source_url"):
                sections.append(f"Source code: {entry['source_url']}\n")
            sections.append(data.decode("utf-8-sig") + "\n")
    return "".join(sections)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, help="Explicit synthetic Cargo metadata fixture")
    args = parser.parse_args()
    if args.metadata:
        metadata = json.loads(args.metadata.read_text(encoding="utf-8-sig"))
    else:
        process = subprocess.run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1",
                                  "--features", "ui,clipboard-events", "--filter-platform", "x86_64-pc-windows-msvc"],
                                 cwd=args.root, check=True, capture_output=True, text=True, encoding="utf-8")
        metadata = json.loads(process.stdout)
    text = collect(metadata, Path(__file__).resolve().parent / "license-texts")
    flag_notices = args.root / "assets" / "emoji-flags"
    if flag_notices.is_dir():
        text += "\n" + "=" * 72 + "\nTwemoji regional flag graphics\n"
        for filename in ("ATTRIBUTION.md", "LICENSE-GRAPHICS"):
            text += "\n" + filename + "\n" + (flag_notices / filename).read_text(encoding="utf-8") + "\n"
    with args.output.open("x", encoding="utf-8", newline="\n") as output:
        unicode_data = args.root / "assets" / "unicode"
        if unicode_data.is_dir():
            text += "\nUnicode character and emoji names\n" + (unicode_data / "SOURCE.md").read_text(encoding="utf-8") + "\n" + (unicode_data / "LICENSE.txt").read_text(encoding="utf-8")
        output.write(text)
    print(f"Wrote dependency notices for {len(metadata['packages']) - len(metadata.get('workspace_members', []))} packages")


if __name__ == "__main__":
    main()
