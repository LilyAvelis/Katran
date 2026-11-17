"""Synthetic checks for the in-memory folder embedding cache; no model or index."""
import importlib.util
from pathlib import Path
import sys
import unittest
import tempfile
import sqlite3
import subprocess

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("folder_worker", Path(__file__).resolve().parents[1] / "scripts/folder-semantic/worker.py")
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)

class FakeNumpy:
    float32 = "float32"
    @staticmethod
    def asarray(values, dtype=None):
        assert dtype == "float32"
        return list(values)
    @staticmethod
    def stack(rows, axis=0):
        assert axis == 0
        return rows

class CacheTest(unittest.TestCase):
    def test_cold_reorder_and_single_path_change(self):
        calls = []
        def encode(texts):
            calls.extend(texts)
            return [[text] for text in texts]
        paths = [f"C:/Synthetic/path-{i}" for i in range(130)]
        cache, matrix = worker.refresh_cache(paths, {}, encode, FakeNumpy)
        self.assertEqual(len(calls), 130)
        self.assertEqual(len(matrix), 130)
        reordered = list(reversed(paths))
        cache, matrix = worker.refresh_cache(reordered, cache, encode, FakeNumpy)
        self.assertEqual(len(calls), 130)
        self.assertEqual(matrix[0][0], worker.descriptor(reordered[0]))
        changed = reordered[1:] + ["C:/Synthetic/new"]
        cache, matrix = worker.refresh_cache(changed, cache, encode, FakeNumpy)
        self.assertEqual(len(calls), 131)
        self.assertNotIn(reordered[0], cache)
        self.assertEqual(len(cache), 130)
        self.assertEqual(matrix[-1][0], worker.descriptor(changed[-1]))
        cache, matrix = worker.refresh_cache([], cache, encode, FakeNumpy)
        self.assertEqual(cache, {})
        self.assertIsNone(matrix)

    def test_descriptor_starts_with_windows_basename(self):
        self.assertTrue(worker.descriptor(r"C:\deep\project").startswith("passage: folder name: project path: "))

class DiskCacheTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.path = str(self.base / "index.sqlite3")
        self.paths = ["C:/Synthetic/a", "C:/Synthetic/b"]
        self.one = [1.0] + [0.0] * 383
        self.two = [0.0, 1.0] + [0.0] * 382
        self.vectors = {self.paths[0]: self.one, self.paths[1]: self.two}
        self.disk = worker.DiskCache(self.path, "synthetic-v1")
        self.assertTrue(self.disk.save(self.paths, self.vectors))

    def test_restart_reuse_one_changed_and_prune(self):
        # Independent Python process opens the same data; no model or personal files.
        child = r"""import importlib.util, pathlib, sys
sys.dont_write_bytecode=True
s=importlib.util.spec_from_file_location('w',sys.argv[1]); w=importlib.util.module_from_spec(s); s.loader.exec_module(w)
class N:
 float32='f32'
 @staticmethod
 def asarray(x,dtype=None): return list(x)
d=w.DiskCache(sys.argv[2],'synthetic-v1'); cache,status=d.load(['C:/Synthetic/a','C:/Synthetic/b'],N)
assert status=='loaded' and len(cache)==2
calls=[]
def encode(texts):
 calls.extend(texts); return [[1.0]+[0.0]*383 for _ in texts]
class Fake(N):
 @staticmethod
 def stack(x,axis=0): return x
cache,matrix=w.refresh_cache(['C:/Synthetic/a','C:/Synthetic/b'],cache,encode,Fake)
assert not calls
cache,matrix=w.refresh_cache(['C:/Synthetic/a','C:/Synthetic/c'],cache,encode,Fake)
assert len(calls)==1 and d.save(['C:/Synthetic/a','C:/Synthetic/c'],cache)
"""
        completed = subprocess.run([sys.executable, "-c", child, str(Path(worker.__file__)), self.path], capture_output=True, text=True)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        loaded, status = self.disk.load([self.paths[0],self.paths[1],"C:/Synthetic/c"],FakeNumpy)
        self.assertEqual(status,"loaded")
        self.assertEqual(set(loaded),{self.paths[0],"C:/Synthetic/c"})
        with sqlite3.connect(self.path) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM embeddings").fetchone()[0],2)

    def test_identity_changes_and_corruption_rebuild_in_memory(self):
        self.assertEqual(worker.DiskCache(self.path,"different").load(self.paths,FakeNumpy)[1],"identity_mismatch")
        model = self.base / "model"; tokenizer = self.base / "tokenizer"
        model.write_bytes(b"model-a"); tokenizer.write_bytes(b"tokenizer")
        original = worker.cache_identity(str(model),str(tokenizer),"cpu","1")
        self.assertNotEqual(original,worker.cache_identity(str(model),str(tokenizer),"gpu","1"))
        self.assertNotEqual(original,worker.cache_identity(str(model),str(tokenizer),"cpu","2"))
        descriptor = worker.DESCRIPTOR_VERSION
        try:
            worker.DESCRIPTOR_VERSION = "synthetic-next-descriptor"
            self.assertNotEqual(original, worker.cache_identity(str(model),str(tokenizer),"cpu","1"))
        finally:
            worker.DESCRIPTOR_VERSION = descriptor
        tokenizer.write_bytes(b"tokenizer-changed")
        self.assertNotEqual(original,worker.cache_identity(str(model),str(tokenizer),"cpu","1"))
        tokenizer.write_bytes(b"tokenizer")
        Path(str(model) + "_data").write_bytes(b"sidecar")
        self.assertNotEqual(original,worker.cache_identity(str(model),str(tokenizer),"cpu","1"))
        Path(str(model) + "_data").unlink()
        model.write_bytes(b"model-b")
        self.assertNotEqual(original,worker.cache_identity(str(model),str(tokenizer),"cpu","1"))
        with sqlite3.connect(self.path) as db:
            db.execute("UPDATE embeddings SET vector=zeroblob(1536) WHERE key=(SELECT key FROM embeddings LIMIT 1)")
        self.assertEqual(self.disk.load(self.paths,FakeNumpy)[1],"corrupt")
        self.assertTrue(self.disk.save(self.paths,self.vectors))
        self.assertEqual(self.disk.load(self.paths,FakeNumpy)[1],"loaded")

    def test_interrupted_write_lock_and_bad_path_preserve_prior_state(self):
        invalid = dict(self.vectors); invalid[self.paths[1]] = [float("nan")] * 384
        self.assertFalse(self.disk.save(self.paths,invalid))
        self.assertEqual(self.disk.load(self.paths,FakeNumpy)[1],"loaded")
        lock = sqlite3.connect(self.path,timeout=0.1)
        try:
            lock.execute("BEGIN EXCLUSIVE")
            self.assertEqual(self.disk.load(self.paths,FakeNumpy)[1],"unavailable")
            self.assertFalse(self.disk.save(self.paths,self.vectors))
        finally:
            lock.rollback(); lock.close()
        self.assertEqual(self.disk.load(self.paths,FakeNumpy)[1],"loaded")
        invalid_path = self.base / "regular-file"
        invalid_path.write_text("not a directory")
        self.assertFalse(worker.DiskCache(str(invalid_path / "index.sqlite3"),"synthetic-v1").save(self.paths,self.vectors))
        self.assertFalse(self.disk.save(["a"] * (worker.MAX_ITEMS+1),{}))
        self.assertEqual(self.disk.load(["a"] * (worker.MAX_ITEMS+1),FakeNumpy)[1],"unavailable")

if __name__ == "__main__":
    unittest.main()
