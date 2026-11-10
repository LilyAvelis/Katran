"""Measure QuickLook CLI and Katran stdio request/title latency on Windows.

Uses only synthetic fixtures; no QuickLook pipe access. Window title is a proxy,
not content-ready. Supply --katran pointing to the build to assess.
"""
import argparse, ctypes, csv, json, os, pathlib, subprocess, tempfile, threading, time
from ctypes import wintypes

p = argparse.ArgumentParser()
p.add_argument('--katran', required=True)
p.add_argument('--fixtures', required=True)
p.add_argument('--quicklook', default=os.path.expandvars(r'%LOCALAPPDATA%\Programs\QuickLook\QuickLook.exe'))
p.add_argument('--pid', required=True, type=int, help='existing QuickLook server PID')
p.add_argument('--rounds', type=int, default=6)
p.add_argument('--output', required=True)
a = p.parse_args()
files = [pathlib.Path(a.fixtures) / x for x in ('text-plain.txt', 'document-synthetic-one-page.pdf', 'image-color-card.png')]
for f in files:
    if not f.is_file(): raise SystemExit(f'Missing fixture: {f}')
user32 = ctypes.WinDLL('user32', use_last_error=True)
user32.EnumWindows.argtypes = [ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM), wintypes.LPARAM]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
callback = user32.EnumWindows.argtypes[0]
def titles():
    found = []
    @callback
    def visit(hwnd, _):
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == a.pid and user32.IsWindowVisible(hwnd):
            n = user32.GetWindowTextLengthW(hwnd)
            buf = ctypes.create_unicode_buffer(n + 1)
            user32.GetWindowTextW(hwnd, buf, len(buf))
            found.append(buf.value)
        return True
    user32.EnumWindows(visit, 0)
    return found

def request(proc, command):
    proc.stdin.write(json.dumps({'api_version': 1, 'request_id': 'bench', 'command': command}) + '\n')
    proc.stdin.flush()
    result = json.loads(proc.stdout.readline())
    if not result.get('ok'): raise RuntimeError(str(result))
    return result

rows = []
with tempfile.TemporaryDirectory(prefix='katran-quicklook-benchmark-') as tmp:
    api = subprocess.Popen([a.katran, 'serve-stdio', tmp], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8')
    try:
        ids = {str(f): request(api, {'type': 'ingest_path', 'path': str(f), 'source': 'synthetic-benchmark'})['result']['id'] for f in files}
        for mode in ('cli', 'api'):
            for i in range(a.rounds * len(files)):
                f = files[i % len(files)]
                start = time.perf_counter()
                result = {}
                def launch():
                    try:
                        if mode == 'cli':
                            child = subprocess.Popen([a.quicklook, str(f)])
                            result['return_ms'] = (time.perf_counter() - start) * 1000
                            result['exit_code'] = child.wait(timeout=3)
                            result['exit_ms'] = (time.perf_counter() - start) * 1000
                        else:
                            response = request(api, {'type': 'preview_external', 'id': ids[str(f)]})
                            result['return_ms'] = (time.perf_counter() - start) * 1000
                            result['response'] = response.get('result')
                    except Exception as exc:
                        result['error'] = repr(exc)
                worker = threading.Thread(target=launch)
                worker.start()
                visible_ms = None
                last_titles = []
                while time.perf_counter() - start < 3:
                    last_titles = titles()
                    if any(f.name.casefold() in t.casefold() for t in last_titles):
                        visible_ms = (time.perf_counter() - start) * 1000
                        break
                    time.sleep(.005)
                worker.join(timeout=4)
                row = {'mode': mode, 'round': i // len(files) + 1, 'file': f.name, 'bytes': f.stat().st_size, 'visible_title_ms': visible_ms, 'observed_titles': last_titles, **result}
                rows.append(row)
                print(json.dumps(row, ensure_ascii=True), flush=True)
                time.sleep(.15)
    finally:
        api.stdin.close()
        api.wait(timeout=5)
# Toggle last target once to close preview through the documented CLI.
subprocess.run([a.quicklook, str(files[-1])], timeout=3, check=False)
out = pathlib.Path(a.output)
out.write_text(json.dumps({'quicklook': a.quicklook, 'katran': a.katran, 'server_pid': a.pid, 'rows': rows}, ensure_ascii=False, indent=2), encoding='utf-8')
with out.with_suffix('.csv').open('w', newline='', encoding='utf-8') as fp:
    fields = ['mode', 'round', 'file', 'bytes', 'return_ms', 'exit_ms', 'exit_code', 'visible_title_ms', 'observed_titles', 'response', 'error']
    writer = csv.DictWriter(fp, fieldnames=fields)
    writer.writeheader()
    writer.writerows(rows)
