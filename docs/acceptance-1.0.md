# 1.0 · user acceptance

Date: 2026-11-09 (Europe/Moscow).
Prepared working build: Shelf 03, `d37d04f69c02`.

The owner considers the basic 1.0 workflow ready for daily use. She personally
confirmed copying selected images into Telegram and Afio, multi-item shelf
deletion, and corrected scrolling. This is user acceptance of those workflows,
not a published release or a claim that all planned architecture is implemented.

Remaining observed issue: image thumbnails load slowly on startup. Visual
polish, including spacing around the capture indicator, is not a release blocker
in this feedback. The owner requests an independent Azuros review of all code,
architectural chokepoints, performance and scalability before choosing 2.0 work.

Review status: completed by Azuros (Terra xhigh). See
[independent report](reviews/azuros-1.0.md). The review preserves the user's 1.0
acceptance and separates integrity/recovery risks before broad distribution from
performance and scale work. Source code was not changed. Synthetic tests and
benchmarks were used; real clipboard contents and desktop were not accessed.
The existing capture test queried the Windows clipboard sequence number only.
