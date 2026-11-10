# Cached upstream license texts

Some locked crates omit upstream license files from their published archive.
`index.json` supplies their exact package versions, original URL and SHA-256.
Git repository revisions come from each cached crate's `.cargo_vcs_info.json`.
Texts are copied verbatim. The selectors MPL 2.0 text comes from Mozilla's
official license page; its exact source revision is recorded separately.

The offline collector checks each stored hash and includes original font notices
from epaint_default_fonts' cached fonts directory. New dependency versions with
missing notices fail until their upstream license texts are reviewed. No network
is used by the collector or CI notice generation.
