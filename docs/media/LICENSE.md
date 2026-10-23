# Example media attribution

These example media are **not covered solely by the repository MIT license**.
The source recordings retain their original licenses below. Cropping, rescaling,
mixing, suppression, spectrogram rendering and lossy video encoding are changes
made by the Quietweft contributors. No endorsement by the original creators is
implied. The composite example audio/video is distributed under
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/),
with the piano recording's original CC BY 3.0 terms and attribution also retained.
Preserve this attribution when sharing the examples.

## Speech, both examples

- File: `4236-8927-0054.flac`, LibriSpeech `train-clean-360`.
- Reader: **Nicholas Feulner**, speaker 4236; chapter 8927, Chapter 47 of
  **The Woodlanders**, by **Thomas Hardy**. LibriSpeech metadata links this
  chapter to book ID 482 and LibriVox project ID 3027.
- Dataset prepared by **Vassil Panayotov**, with assistance from **Daniel Povey**,
  derived from LibriVox public-domain audiobook recordings.
- Source and corpus license: [LibriSpeech, OpenSLR 12](https://www.openslr.org/12/),
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
- Changes: source time 0.5–12.5 seconds, rescaled and mixed; output processed
  with GTCRN, then encoded as AAC and rendered as a spectrogram.

## Piano, speech-music example

- MUSAN file: `music-hd-0039.wav`, **Gymnopedie No. 1**, composed by **Erik Satie**,
  performed by **Kevin MacLeod** (the MUSAN metadata spells this “Kevin MacLoad”).
- License: [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/).
- Source listed by the corpus:
  [HD Classical Music recording 512](http://www.hdclassicalmusic.com/listen/?id_song=512).
  Performer: [Incompetech](https://incompetech.com/).
- License evidence: individual entry in MUSAN
  `music/hd-classical/LICENSE`, not a blanket license for every MUSAN file.
- Changes: source time 30–44 seconds, rescaled, mixed with speech; output
  processed with GTCRN, encoded and visualized.

## Hammering, speech-impacts example

- MUSAN file: `noise-sound-bible-0035.wav`, **Hammering**, recorded by **Lisa Redfern**.
- License stated in the individual corpus entry: **Public Domain**.
- Source: [SoundBible — Hammering](http://soundbible.com/2082-Hammering.html).
- License evidence: individual entry in MUSAN `noise/sound-bible/LICENSE`.
- Changes: source time 0–2.8 seconds, placed at scene times 2.0 and 8.8 seconds,
  rescaled and mixed; output processed with GTCRN, encoded and visualized.

MUSAN distribution: [OpenSLR 17](https://www.openslr.org/17/).
Machine-readable source and model hashes: [provenance.json](provenance.json).
