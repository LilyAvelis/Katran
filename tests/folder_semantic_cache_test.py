"""Synthetic checks for the in-memory folder embedding cache; no model or index."""
import importlib.util
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("folder_worker", Path(__file__).resolve().parents[1] / "scripts/folder-semantic/worker.py")
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)

class FakeNumpy:
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

if __name__ == "__main__":
    unittest.main()
