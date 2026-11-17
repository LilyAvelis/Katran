# Folder semantic search: optional NVIDIA runtime

The Folder neural search can optionally use a separate Windows NVIDIA CUDA runtime. It is independent of the existing emoji AI Python environment; the validated model and tokenizer are reused read-only. CPU mode and ordinary Everything search do not require the optional GPU runtime.

For a source checkout on Windows with Python 3.10, install the component into an existing Katran profile:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/folder-semantic/setup-gpu.ps1 -Profile <profile-directory>
```

The installer creates only `folder-ai-gpu-runtime` within that profile, installs pinned ONNX Runtime GPU 1.23.2, CUDA 12 and cuDNN 9 packages into a separate venv, and writes its completion marker after dependency validation. It does not install or update system drivers/CUDA, modify emoji AI files, or download model weights. Installation downloads several large NVIDIA wheels. A completed runtime is never overwritten by the script; an incomplete installation can be resumed. As with any Python package installation, transitive dependencies are resolved by pip and network access is necessary.

The resolver checks the location, marker and interpreter, but this does not establish that the GPU is usable. The worker preloads local CUDA DLLs and checks the CUDA execution provider; failures must be surfaced explicitly as `folder_gpu_unavailable`. No silent whole-request CPU fallback is promised. If GPU mode cannot start, disable it to use the existing CPU mode while diagnosing the optional runtime.

Compatibility: the ONNX Runtime [CUDA execution provider documentation](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html) describes CUDA 12 and cuDNN 9 support and DLL preloading. Driver, hardware, provider and model compatibility still require a real inference test on the target machine.

## Verified local target · 2026-11-17

The isolated component was installed on Lilia's RTX 5090 (driver 616.86) and
`pip check` passed. Real E5 profiling recorded 349 CUDA nodes, including 84
MatMul and 12 FusedMatMul operations; the two CPU nodes were Gather and Unsqueeze
shape work. A blanket prohibition of every CPU node prevented startup and was
removed. CUDA presence and disabled whole-session fallback are still checked.
Small CPU shape operations are compatible with GPU neural computation.

Single 64-name process observations: CPU startup 981ms, first request 104ms,
cached 3ms; GPU startup 1264ms, first 184ms, cached 3ms. This small case did not
show an acceleration and is not a throughput benchmark.

The separate end-to-end Settings → job → resolver → resident CUDA API probe used
8192 synthetic paths: cold 8544ms, warm 51ms, one-path-change 51ms; all three
intended targets ranked first. The earlier CPU 8192-path probe was 30925ms cold,
but the runs were not paired controlled benchmarks and do not justify a general
speedup multiplier. Real-index and other machines' timings can differ.
No extra model or personal-folder content was downloaded/read for these checks.
