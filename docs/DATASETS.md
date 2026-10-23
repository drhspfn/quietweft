# Dataset preparation

No training corpus is bundled or downloaded automatically. The app's ONNX model
and the official training checkpoint have separate provenance from your audio.
The repository's MIT license does not grant rights to third-party recordings.

Use audio you own or have permission to train on and redistribute if needed.
Record dataset version, source URL, license and any speaker-consent restrictions
outside the generated pairs directory. Publicly downloadable does not mean
unrestricted. MUSAN, DNS and LibriSpeech have their own source terms; inspect the
specific material before use. There is no requirement to fetch an 11 GB archive
to test this pipeline: your own clean voice and noise recordings are sufficient
for a software smoke test.

## Labels matter

- `speech/<speaker>/`: genuinely clean speech. A click in this reference teaches
  the model to retain that click. Do not assume a speech corpus is artifact-free.
- `noise/<source-session>/`: unwanted sounds **without intelligible speech**.
  Marking speech as noise teaches the model to erase speech.
- Put every excerpt of the same noise recording/session in one directory. Use
  different sessions for keyboard, mouse, impacts, traffic, hum, music, dishes,
  ventilation and room tone as available. A filename alone cannot prove source
  independence.
- Keep speakers in separate directories, with all their recordings together.
  At least three groups per role are needed for non-overlapping train/val/test.

The manifest records each original source file's SHA-256 and relative path. It
does not upload audio or include your absolute home directory. Preserve original
source files so the manifest remains auditable. Adding more prepared scenes
reuses the same finite sources; it does not create independent evaluation data.

## Recording useful examples

Include individual key presses and clicks separated by silence, normal typing,
short series, distant and nearby impacts, quiet and loud noises, and recordings
with room reflections. Keep some entire recording sessions and speakers unseen
until final evaluation. For personal microphones, record evaluation separately
from the training session. Never infer general quality from a training example.

The generated curriculum mixes noise into real speech and also trains pure-noise
and pristine cases. It does not currently synthesize room impulse responses,
clipping, device equalization or universal sound-class coverage. Corpus diversity
and independent human listening are still necessary.
