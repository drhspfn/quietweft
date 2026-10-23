"""Auditable source-disjoint, matched speech/noise training examples."""
from __future__ import annotations

import json
import math
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly
from tqdm import tqdm

from .network import sha

EXTENSIONS = {".wav", ".flac", ".ogg"}


def read_audio(path):
    wave, rate = sf.read(path, dtype="float32", always_2d=True)
    wave = wave.mean(1)
    if rate != 16000:
        divisor = math.gcd(rate, 16000)
        wave = resample_poly(wave, 16000 // divisor, rate // divisor).astype("float32")
    if not len(wave) or not np.isfinite(wave).all():
        raise ValueError(f"Invalid audio: {path}")
    return wave


def _groups(root):
    root = Path(root)
    groups = {}
    for path in sorted(root.rglob("*")):
        if path.is_file() and path.suffix.lower() in EXTENSIONS:
            relative = path.relative_to(root)
            group = relative.parts[0] if len(relative.parts) > 1 else relative.as_posix()
            groups.setdefault(group, []).append(path)
    return groups


def _split(groups, rng):
    names = sorted(groups)
    if len(names) < 3:
        raise ValueError("Need at least 3 independent speaker/noise groups for train/val/test")
    rng.shuffle(names)
    reserved = max(1, round(len(names) * .1))
    return {"test": names[:reserved], "val": names[reserved:2*reserved], "train": names[2*reserved:]}


def prepare(speech, noise, output, scenes=4000, seed=42, seconds=(1., 3., 10.)):
    """Each scene creates noise-only, mixed and pristine examples with identical noise."""
    if scenes < 3 or not seconds or min(seconds) < .064:
        raise ValueError("Use at least 3 scenes and positive durations >= 0.064 seconds")
    output = Path(output)
    if output.exists() and any(output.iterdir()):
        raise FileExistsError("Pairs output must be new or empty")
    voices, noises = _groups(speech), _groups(noise)
    rng = np.random.default_rng(seed)
    splits = {"speech": _split(voices, rng), "noise": _split(noises, rng)}
    output.mkdir(parents=True, exist_ok=True)
    source_records = []
    for role, root, groups in (("speech", speech, voices), ("noise", noise, noises)):
        for group, paths in groups.items():
            split = next(s for s, names in splits[role].items() if group in names)
            for path in paths:
                source_records.append({"role": role, "group": group, "split": split,
                    "path": path.relative_to(root).as_posix(), "sha256": sha(path)})
    counts = {"val": max(1, round(scenes * .1)), "test": max(1, round(scenes * .1))}
    counts["train"] = scenes - counts["val"] - counts["test"]
    records = []
    for split in ("train", "val", "test"):
        (output / split).mkdir()
        for index in tqdm(range(counts[split]), desc=f"Prepare {split}", unit="scene"):
            speaker = rng.choice(splits["speech"][split])
            source = rng.choice(splits["noise"][split])
            speech_path = voices[speaker][rng.integers(len(voices[speaker]))]
            noise_path = noises[source][rng.integers(len(noises[source]))]
            size = max(1024, int(rng.choice(seconds) * 16000))
            voice = read_audio(speech_path)
            start = int(rng.integers(max(1, len(voice) - size + 1)))
            clean = np.zeros(size, dtype="float32")
            segment = voice[start:start+size]
            clean[:len(segment)] = segment
            clean *= 10 ** (rng.uniform(-30, -15) / 20) / max(float(np.sqrt(np.mean(clean**2))), 1e-6)
            recording = read_audio(noise_path)
            duration = min(size, len(recording), int(rng.choice([.064, .128, .512, 1., float(size / 16000)]) * 16000))
            event_start = int(rng.integers(max(1, len(recording) - duration + 1)))
            event = recording[event_start:event_start+duration].copy()
            event *= 10 ** (rng.uniform(-45, -12) / 20) / max(float(np.sqrt(np.mean(event**2))), 1e-6)
            stem = np.zeros(size, dtype="float32")
            regions = []
            for _ in range(int(rng.choice([1, 1, 2, 5]))):
                onset = int(rng.integers(max(1, size-duration+1)))
                stem[onset:onset+duration] += event
                regions.append([onset, onset+duration])
            scale = min(1., .95 / max(float(np.max(np.abs(clean + stem))), 1e-6))
            clean *= scale
            stem *= scale
            relative = f"{split}/{index:06d}.npz"
            np.savez_compressed(output / relative, clean=clean, noise=stem)
            records.append({"file": relative, "split": split, "speaker": str(speaker), "noise_group": str(source),
                "speech": speech_path.relative_to(speech).as_posix(), "noise": noise_path.relative_to(noise).as_posix(),
                "samples": size, "event_regions": regions})
    manifest = {"format": "quietweft_pairs_v1", "seed": seed, "sample_rate": 16000,
        "triplets_per_scene": ["noise_only", "mixed", "pristine"],
        "sources": source_records, "splits": splits, "scenes": records}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def triplet(path):
    with np.load(path, allow_pickle=False) as data:
        clean, noise = data["clean"], data["noise"]
    return np.stack([noise, clean + noise, clean]), np.stack([np.zeros_like(clean), clean, clean])
