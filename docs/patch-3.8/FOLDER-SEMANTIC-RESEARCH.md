# Optional semantic Folder search · 2026-11-17

Later owner instruction implements a bounded optional mode; see
[Folder neural mode](FOLDER-NEURAL.md). This report preserves the prior research
and its smaller feasibility probe; those numbers are not production latency.

Research only. The accepted 3.8 ordinary Everything search remains the default.
No Folder AI command, model download, personal-folder index or cloud service was
added. Existing optional emoji AI is unchanged.

## Recommendation

First evaluate the already installed multilingual E5 encoder in an opt-in,
separate Folder experiment. It can match intent across Russian/English names;
it does not replace exact names, fuzzy matching or Everything. A neural reranker
cannot discover a folder absent from its candidate set. Returning a better order
over the same truncated lexical matches is different from semantic discovery.

For discovery, define user-selected roots and a bounded background index of
folder names and paths, optionally favorites/history. Do not read document
contents or silently enumerate the entire disk. Keep literal exact hits first
and ordinary results usable if the model is absent, slow or fails. Expose index
scope, incompleteness and stale/deleted entries; validate a chosen path through
the existing Folder command before navigation.

## Models and existing runtime

| Choice | Evidence and fit |
| --- | --- |
| Existing `intfloat/multilingual-e5-small` | MIT, 384 dimensions, multilingual; retrieval uses `query:` and `passage:` prefixes even for non-English text. Model card caps input at 512 tokens. The current Katran encoder deliberately caps at 128. Best first feasibility experiment because weights/runtime are already present. [Model card](https://huggingface.co/intfloat/multilingual-e5-small) |
| `Qwen/Qwen3-Embedding-0.6B` | Apache-2.0, 0.6B parameters, 32K context, configurable embedding dimension up to 1024 and multilingual coverage. A possible later comparison, not a proven upgrade for short folder names. Requires a different runtime/export and measured memory/latency. No download was made. [Model card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) |

The local `scripts/emoji-ai/semantic_search.py` encoder runs ONNX on CPU, mean
pools and normalizes vectors, and assumes 384 dimensions. Reusing its weights
and encoding code is feasible. Reusing the emoji worker protocol as if it were
a folder API is not: protocol v1 returns emoji catalogue indices, has bounded
frames and queries, and Rust validates emoji semantics. Keep a separate bounded
Folder request/result contract and derived index/cache identity; do not overwrite
emoji manifests, indexes or deployed worker scripts. Model identity, tokenizer,
prefixes, normalization, dimensions and descriptor version must invalidate a
derived index when changed. Existing integrity checks remain necessary.

At float32, 384 dimensions cost 1,536 bytes per folder: about 146.5 MiB for
100,000 vectors alone, before paths, metadata and runtime. Choose scope and an
index design from measured scale, rather than adding a vector database by default.

## Local feasibility probe

One offline probe used the existing pinned model/tokenizer and profile Python
with downloads disabled. Only 16 synthetic folder paths were encoded; no real
folder index or file content was read. Four CPU threads, CPUExecutionProvider.
Ignored evidence: `target/research/folder_semantic_probe.py` and
`target/research/folder-semantic-probe.json`.

- Encoder initialization: **1,206.27 ms**, excluding preceding hash checks and
  process launch. Encoding the 16 synthetic descriptors: **19.82 ms**.
- Five hand-written Russian/English queries each placed the intended synthetic
  target first, including travel photos, invoices, florist workshop, a Rust image
  shelf and `cat run` → Katran.
- Warm query encoding alone: **2.28–2.98 ms**, one observation per query.

These are feasibility observations, not production quality or end-to-end latency.
The tiny easy corpus has no held-out evaluation; cache state was not controlled.
Times exclude Everything, candidate acquisition, index maintenance, startup,
cancellation, IPC and UI. They do not establish p95, full-disk recall, a perfect
success rate or superiority over the existing ordinary search.

Pinned model SHA-256:
`ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665`.
Tokenizer SHA-256:
`0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39`.

## Implementation gate before choosing a model

1. Agree discovery scope and whether the feature reranks lexical candidates or
   discovers semantically within an explicit index. Keep ordinary search intact.
2. Build a meaningful synthetic/owner-approved RU/EN query set with exact names,
   typos, transliteration, ambiguous intents, unrelated queries, duplicate names,
   long paths and deleted/moved folders. Compare ordinary vs hybrid top-k recall
   and wrong-result rate; do not hardcode known project names.
3. Domain API first: explicit optional mode, cancellation/generation, bounded
   jobs and results, deterministic lexical fallback, honest partial/stale states,
   model absent/corrupt/timeout behavior. No paths leave the computer.
4. Measure cold/warm end-to-end latency, p50/p95, process memory and index cost
   over an explicitly permitted realistic scope. Test fast query changes and
   restart/cache invalidation before adding GUI controls.

The owner's decision to connect neural Folder search is still pending. The
current evidence supports a small local experiment, not an automatic integration.
