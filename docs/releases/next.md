# Unreleased local changes after 3.4.1

- Actions can set one selected image as Windows desktop wallpaper. The domain command validates the current selection, stores an immutable managed PNG, then asks Windows to use it on all monitors. The documented API does not specify a virtual-desktop target; current-desktop and video wallpaper commands are not offered.
- General offers main-window resizing. It is disabled by default, persists in settings, and applies after saving and at startup. Layouts remain optimized for the default window size.
