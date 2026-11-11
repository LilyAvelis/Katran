#!/usr/bin/env python3
"""Validate and stage the pinned optional FFmpeg distribution without executing it."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import zipfile

SHA256 = '4ac2a4a732ee66707ab2a4d7fc135d085e1da691f365cdb2f1b48f8f3816d4d8'
SIZE = 76530311
ROOT = 'ffmpeg-n9.0.2-49-gc179411bbb-win64-lgpl-shared-9.0/'
URL = 'https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/autobuild-2026-11-11-12-39/ffmpeg-n9.0.2-49-gc179411bbb-win64-lgpl-shared-9.0.zip'

def stage(archive, destination, expected_hash=SHA256, expected_size=SIZE, prefix=ROOT):
    if archive.stat().st_size != expected_size:
        raise ValueError('Unexpected media runtime size')
    with archive.open('rb') as source:
        if hashlib.file_digest(source, 'sha256').hexdigest() != expected_hash:
            raise ValueError('Media runtime SHA-256 mismatch')
    with zipfile.ZipFile(archive) as z:
        names = z.namelist()
        if len(names) != len(set(names)):
            raise ValueError('Duplicate archive entries')
        selected = {}
        for entry in z.infolist():
            name = entry.filename
            if not name.startswith(prefix):
                raise ValueError('Unexpected archive root')
            rel = name[len(prefix):]
            if entry.is_dir():
                continue
            if rel == 'LICENSE.txt':
                dest = 'LICENSE.txt'
            elif rel.startswith('bin/') and '/' not in rel[4:] and rel[4:].lower().endswith(('.dll', '.exe')):
                dest = 'tools/' + rel[4:]
            else:
                continue
            if dest.lower() in selected or entry.file_size > 200 * 1024 * 1024:
                raise ValueError('Duplicate or oversized runtime entry')
            selected[dest.lower()] = (dest, entry)
        if not {'tools/ffmpeg.exe', 'tools/ffprobe.exe', 'license.txt'} <= selected.keys():
            raise ValueError('FFmpeg executables or license missing')
        if not any(name.endswith('.dll') for name in selected):
            raise ValueError('Shared FFmpeg DLLs missing')
        if sum(entry.file_size for _, entry in selected.values()) > 512 * 1024 * 1024:
            raise ValueError('Media runtime expanded size exceeds limit')
        destination.mkdir(parents=True, exist_ok=False)
        for dest, entry in selected.values():
            path = destination / dest
            path.parent.mkdir(parents=True, exist_ok=True)
            with z.open(entry) as source, path.open('xb') as out:
                shutil.copyfileobj(source, out)
        (destination / 'PROVENANCE.json').write_text(json.dumps({'source': URL, 'sha256': expected_hash, 'license': 'LGPL shared build', 'source_project': 'https://github.com/FFmpeg-Builds/FFmpeg-Builds'}, indent=2) + '\n', encoding='utf-8')

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('archive', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    stage(args.archive, args.destination)
