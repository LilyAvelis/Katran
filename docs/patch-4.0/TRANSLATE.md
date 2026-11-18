# Translate foundation

Translate is a dedicated shelf view for managed images explicitly published
as translation results. Catalogue v14 adds the result link and a separate tag
scope. Ordinary clipboard images, upscale results and original inputs do not
gain Translate membership through their filename or source label.

`crates/baloh-collection` is a separate Rust library for ordered payloads
with stable entry keys. An image translation job will use one entry per
original page. It can fill each entry's result or error independently while
preserving the input order. Katran owns image storage, job state and shelf
publication. The collection library is independent of the Katran catalogue
and may later be reused by Avilon or another project.

The collection library defines in-memory behavior. File encoding, validation
of standalone .bull archives, and QuickLook are later work. A translation
batch should reopen as a single ordered result, with successful pages and
per-page failures associated with their originals.

## First functional gate

Russian and English are the primary translation languages. Recognition must
also cover Japanese, Korean, and Chinese. Start with one page containing
identified regions and a known reading order. Record the detected regions,
transcription, ordered dialogue, translation, cleaned background and final
lettering as independently inspectable intermediate results. Evaluate vertical
and right-to-left page order with a separately labelled manga example.

[PP-OCRv5](https://www.paddleocr.ai/v3.6.0/en/version3.x/algorithm/PP-OCRv5/PP-OCRv5_multi_languages.html)
is a candidate recognizer with models for these languages. A model's
listed language is coverage evidence only; the project still needs a real
page run to verify mixed scripts and vertical dialogue. The local ComfyUI
environment already has PyTorch, Transformers and OpenCV, but no working
OCR and inpainting translation pipeline has been verified. Select the
translation backend and explicit optional component setup after the fixtures.

The Translate shelf can be opened before any results exist. It displays
only accepted managed outputs once that publishing path is implemented.
