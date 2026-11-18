# Static image upscale

Select a static image, including a frame previously saved from a video,
multiple images, or one image BULL.
Ctrl+K → AI Commands offers Remacri ×2, Remacri ×4, Real-ESRGAN ×2, and
Real-ESRGAN ×4. Each is individually rebindable and launches directly. The
result is a separate managed PNG in Images for a single loose image.
For multiple images or a BULL, the result is a new BULL in Images. Every
page uses the same model and factor. The receipt reports successes and
failed input indices, and the saved pages keep their input order. The
original images and any input collection are unchanged.

Each dimension is exactly multiplied by the requested factor after applying
the input's EXIF orientation. Remacri uses `4x_foolhardy_Remacri.pth` and
resamples its ×4 output for ×2. Real-ESRGAN uses `RealESRGAN_x2plus.pth`
and `RealESRGAN_x4.pth` respectively. Transparent input retains a scaled
alpha channel. Video and animated images are outside scope.

## Discovery

The existing Settings → AI page reports local availability. It checks
`ai-models/upscale_models` under the active profile first, then the standard
ComfyUI directory and its explicit `extra_model_paths.yaml` upscale paths.
Setup normally uses `%LOCALAPPDATA%\Katran\data`. Portable normally uses
`katran-data` beside its launcher. The worker uses the Python environment
inside existing ComfyUI Windows Portable. Set `KATRAN_COMFYUI_ROOT` to the
portable directory for a nonstandard installation. The default is
`%SystemDrive%\ComfyUI\ComfyUI_windows_portable`.

The worker needs PyTorch, Spandrel, Pillow, and NumPy. It uses CUDA when
available and CPU otherwise. Model presence in Settings only confirms
discovery. No model or runtime is downloaded or copied. No ComfyUI server
or graph execution is needed. PyTorch opens the weights in weights-only mode.

## Boundaries

`UpscaleSelectedImage` binds the selected revision at submission and
publishes through normal managed-image storage. The worker runs in a separate
process, with one active upscale job. Cancellation before publication leaves
no result. The input limit is 32 MiB; output is limited to 8192 pixels per
side and 32 million pixels. The worker also bounds the native intermediate.
AI inference is tiled, so texture continuity at tile seams needs visual
acceptance on representative images.
