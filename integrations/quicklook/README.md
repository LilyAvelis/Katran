# Optional external QuickLook backend source

QuickLook is a separate GPL application, not embedded into Katran's executable.
This directory's upstream-derived patch is GPL-3.0; the GPL and Common notices
here apply to this component instead of Katran's MIT license. Preserve upstream
notices. SOURCE-PROVENANCE.json records the exact upstream/common commits and
matching complete source archive hash distributed alongside the installed viewer.
No compiled viewer/plugin binaries or personal UserData are checked into Git.

To reproduce the source, clone QL-Win/QuickLook, checkout commit
be7f0f3dc9a2f782b67d19e98a3ba7651206b5e6 (tag4.4.0), initialize its
QuickLook.Common submodule and checkout9dda0f93dae34d0337ad39220733c254715fd09d
there. From the QuickLook root apply quicklook-4.4.0-katran-bridge.patch with git
apply. Build normal release with:

    dotnet build QuickLook/QuickLook.csproj -c Release -p:PreBuildEvent= -v:q

Do not define KATRAN_BRIDGE_LAB for a normal package. Stock4.4 plugin binaries
and their original notices are separate distribution inputs; their implementations
were not changed. The installed package includes the matching complete application
and Common source ZIP, notices and provenance. The production executable accepted
on this host has SHA256
b8c1bfb001270be5d4551467f0df014ee98f0bca01bfea300d3562da6f2a6ca6.

See docs/quicklook-fast-backend.md for protocol, lifecycle and validation boundaries.
