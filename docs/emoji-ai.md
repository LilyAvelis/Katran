# Emoji AI search · optional local model

Katran's Pods catalogue includes ordinary text search without any model. AI search is a separate mode in the same catalogue: click **Поиск с ИИ** or press `Ctrl+Shift+T`, type a query and press Enter. Results can be selected, copied into Pods, pasted into the previous window, or dragged out like other catalogue emoji. `Ctrl+T` returns to ordinary search.

The app does not download a model automatically. Without a model, the AI mode shows **Модель не установлена** and the search command returns `model_unavailable`; the rest of Katran remains available. There is no account or payment check in the 3.7 line. Authorization is deferred to 4.0.

## Install on Windows

Run the installer only if you want local AI search. Use PowerShell from a Katran source checkout or a portable package that contains `scripts/install-emoji-ai-model.ps1`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install-emoji-ai-model.ps1 -Profile "C:\path\to\Katran\katran-data"
```

Pass the actual Katran profile directory. Portable installations normally use `katran-data` next to `Katran.exe`; per-user setup uses `%LOCALAPPDATA%\Katran\data`. The script needs Python 3.10 x64 and internet access for Python packages and the model. It installs packages in a private virtual environment under the profile, downloads the pinned model and tokenizer, builds the English/Russian emoji index, checks their SHA-256 values and writes an integrity manifest. It stages the runtime before moving it to `<profile>\emoji-ai-runtime`; it refuses to overwrite an existing runtime. The app keeps finding this directory after updates.

If the Python launcher cannot find 3.10, pass a verified 64-bit Python 3.10 executable with `-Python "C:\path\to\python.exe"`. Even with local model files, package installation still needs an internet connection or a configured local Python package source.

If you already have the two verified files, avoid another model download:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install-emoji-ai-model.ps1 -Profile "C:\path\to\Katran\katran-data" -ModelFile "C:\path\to\model.onnx" -TokenizerFile "C:\path\to\tokenizer.json"
```

The model is [intfloat/multilingual-e5-small](https://huggingface.co/intfloat/multilingual-e5-small/tree/614241f622f53c4eeff9890bdc4f31cfecc418b3), pinned to revision `614241f622f53c4eeff9890bdc4f31cfecc418b3`. Its [ONNX model file](https://huggingface.co/intfloat/multilingual-e5-small/blob/main/onnx/model.onnx) is about 470 MB and the model card lists the MIT license. The expected SHA-256 of `model.onnx` is `ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665`; the tokenizer is `0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39`. The local index and worker use the bundled Unicode 16 emoji catalogue and [Unicode CLDR](https://cldr.unicode.org/) English/Russian annotations. Search queries remain local to the worker; Katran does not submit them to Hugging Face.

The current runtime format still has the historical `local-experiments/emoji-ai` directory names inside its private profile folder. These names are an internal compatibility detail, not a second application. For a custom runtime location, set `KATRAN_EMOJI_AI_RUNTIME_ROOT` to its absolute path before launching Katran. An explicitly selected but invalid runtime fails with an error rather than silently falling back elsewhere.

## Troubleshooting

- `model_unavailable`: install the model into the active profile with the script above. Verify that the profile path matches the one passed to Katran at launch.
- `emoji_ai_unavailable`: files are present but the runtime is incomplete, changed, or cannot start. Reinstall into a new profile directory after inspecting the reported error; the installer does not replace existing files.
- AI search can take longer on the first request while the local worker starts and verifies the model. Typing does not launch inference; Enter does.

The source and installation helper are public, but model weights, virtual environment and generated index are deliberately excluded from Git and Katran packages. A full clean-machine installation check remains required before publishing the optional installer as supported distribution.
