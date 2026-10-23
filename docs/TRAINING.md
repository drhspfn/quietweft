# Train a candidate

The app ships the pinned, pretrained GTCRN DNS3 model. Training is optional and
experimental. Our earlier fine-tuning experiments did **not** reliably improve
that baseline. A lower spectral loss is not evidence of better speech quality.
These commands never replace the release model or download a dataset.

Run commands from a checkout of this repository, using Python 3.11 or newer:

```sh
uv sync --extra test
uv run quietweft-train --help
```

`uv` creates a local environment. PyTorch is a large dependency; installing the
environment can take several minutes. For a CUDA build, follow the installation
command for your GPU on [PyTorch's official installer](https://pytorch.org/get-started/locally/)
and use that environment's `python -m quietweft.train`. The CLI does not install
GPU drivers or change your PyTorch build. `--device cuda` fails clearly when CUDA
is unavailable; `auto` selects CUDA if supported, otherwise CPU.

## 1. Add audio

```text
data/
  speech/
    speaker-001/  # clean speech, no keyboard/music/clicks
      take.wav
    speaker-002/
      take.wav
    speaker-003/
      take.wav
  noise/
    keyboard-session-001/  # noise only, no speech
      take.wav
    music-recording-001/
      take.flac
    traffic-recording-001/
      take.wav
```

Use at least **three independent speakers and three independent noise source
groups**. All excerpts from the same source/session belong in one group. More
speakers, sessions, microphones and rooms are strongly preferable. WAV, FLAC and
OGG are accepted, converted to mono 16 kHz. Keep your original recordings.
See [Dataset preparation](DATASETS.md) for licensing and labeling guidance.

## 2. Prepare matched examples

```sh
uv run quietweft-train prepare --speech data/speech --noise data/noise --output data/pairs --scenes 4000 --seed 42
uv run quietweft-train inspect data/pairs
```

`--scenes` counts matched triplets. Each triplet contains noise alone → silence,
the same noise plus clean speech → clean speech, and clean speech → itself.
Short noise excerpts (64 ms through full-scene length), isolated/series events,
different levels and event positions are generated within 1, 3 and 10 second
scenes. Override with `--seconds 1 3 10`. A short recording can leave trailing
silence; it is never looped to fabricate a longer voice recording.

Speakers and noise source groups are assigned to train/validation/test before
mixing; no source crosses these splits. `manifest.json` records seed, relative
source filenames, SHA-256 hashes, split membership and event sample intervals.
The test split is reserved; `fit` never uses it for gradients or early stopping.
Output must be empty, preventing accidental replacement of an existing dataset.

The grouping policy treats a noise subdirectory as one source group. A loose
noise file is a separate group. Put related excerpts in one subdirectory to
avoid leakage. A session split is not proof of generalization to another room.

## 3. Fine-tune

```sh
uv run quietweft-train fit --pairs data/pairs --output runs/first --epochs 100 --batch 12 --device auto --lr 0.00001 --patience 12
```

The model starts from the bundled official, trained DNS3 checkpoint. Fixed ERB
matrices and BatchNorm running statistics stay fixed; trainable network weights
receive gradients. The loss combines level-normalized, compressed complex and
magnitude spectral errors, including silence targets. SI-SNR is not used for
silent targets. Gradients are clipped. `--batch` is an example budget rounded
down to complete triplets (minimum one triplet). Reduce it if GPU memory fills.

Progress bars show batches and losses. Each completed epoch atomically saves
`last.pt`; an improved validation loss also saves `best.pt`. `history.jsonl`
contains epoch losses and durations. `config.json` records hyperparameters and
upstream pins. After `--patience` epochs without an improvement of at least
0.00001, training stops. Runtime depends on corpus and hardware; there is no
promised epoch count or quality threshold.

Continue from the last completed epoch in the **same run directory**:

```sh
uv run quietweft-train fit --pairs data/pairs --output runs/first --resume runs/first/last.pt --epochs 150 --batch 12 --device auto --lr 0.000005 --patience 20
```

`--epochs` is the total epoch limit, not an additional count. Resume restores
optimizer state and random generators, checks the dataset manifest hash, and
uses the explicitly supplied learning rate. Stopping mid-epoch retains the
last completed epoch, not partial gradients. Checkpoints are local trusted
artifacts; do not load unknown checkpoint files.

## 4. Export explicitly

```sh
uv run quietweft-train export --checkpoint runs/first/best.pt --output models/candidates/first
```

A checkpoint with at least one actual training step is required. There is no
random-weight fallback and no implicit download. The export produces
`model.onnx`, `manifest.json`, `numerical-parity.json` and the upstream license.
It compares batch PyTorch, frame-by-frame PyTorch, ONNX Runtime audio and all
recurrent caches on a seeded noise/silence/impulse probe. A parity failure raises
an error and does not create a usable manifest. Output must be empty.

This verifies conversion, **not sound quality**. Candidate manifests explicitly
say `quality_approved: false`. Listen to unseen clean speech, noise-only and
mixed scenes, including quiet consonants, single clicks, music, late onsets,
different rooms and microphones. Compare against the unchanged release model,
at the same playback volume. Audit clicks already present in supposedly clean
speech references before interpreting a reference error as speech damage.

Candidates use the same `gtcrn_generic_stream_v1` ABI and one global suppression
control. They are not class-selective denoisers. Integrators can load a candidate
manifest through the runtime API or CLI model option; distribution/replacement
of a candidate remains an explicit separate action.

## Verify the tools

```sh
uv run pytest tests/python -q
```

The tests generate synthetic fixtures and perform CPU training, checkpoint
resume, streaming ONNX export and ONNX Runtime parity. They also check split
disjointness, matched silence targets, fixed ERB parameters and rejection of an
untrained checkpoint. These fixtures are software checks, not quality evidence.

