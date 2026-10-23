"""Prepare your own audio, fine-tune explicitly, and export a verified candidate."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import time

import numpy as np
import torch
from tqdm import tqdm

from .data import prepare, triplet
from .network import load_model, sha, spectrum, verify_vendor


def spectral_loss(predicted, desired, mixture):
    """Compressed complex/magnitude loss remains defined for silence targets."""
    power = mixture.abs().square().detach()
    weights = power / power.sum((-2, -1), keepdim=True).clamp_min(1e-12)
    magnitude = predicted.abs().clamp_min(1e-6)
    target = desired.abs().clamp_min(1e-6)
    complex_error = (predicted / magnitude.pow(.7) - desired / target.pow(.7)).abs().square()
    magnitude_error = (magnitude.pow(.3) - target.pow(.3)).square()
    scale = power.mean((-2, -1)).clamp_min(1e-10).pow(.3)
    return (((.3 * complex_error + .7 * magnitude_error) * weights).sum((-2, -1)) / scale).mean()


def _batches(rows, pairs, batch):
    # A batch contains complete matched triplets; padding preserves zero targets.
    scenes_per_batch = max(1, batch // 3)
    for offset in range(0, len(rows), scenes_per_batch):
        arrays = [triplet(pairs / row["file"]) for row in rows[offset:offset+scenes_per_batch]]
        length = max(x.shape[-1] for x, _ in arrays)
        yield tuple(torch.from_numpy(np.concatenate([np.pad(pair[i], ((0, 0), (0, length-pair[i].shape[-1])))
            for pair in arrays])) for i in (0, 1))


def fit(pairs, output, epochs=100, batch=12, device="auto", resume=None, patience=12, lr=1e-5, seed=42):
    if min(epochs, batch, patience) < 1 or not 0 < lr < 1:
        raise ValueError("Epochs, batch and patience must be positive; learning rate must be in (0,1)")
    pairs, output = Path(pairs), Path(output)
    if resume and Path(resume).resolve().parent != output.resolve():
        raise ValueError("Resume into the original run directory to retain its best checkpoint and history")
    if not resume and output.exists() and any(output.iterdir()):
        raise FileExistsError("Training output must be new or empty unless --resume is provided")
    manifest_path = pairs / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("format") != "quietweft_pairs_v1":
        raise ValueError("Unsupported pairs manifest")
    data_hash = sha(manifest_path)
    if device == "auto":
        device = "cuda" if torch.cuda.is_available() else "cpu"
    if device == "cuda" and not torch.cuda.is_available():
        raise ValueError("CUDA is unavailable in this PyTorch installation; use --device cpu")
    torch.manual_seed(seed)
    model, checkpoint = load_model(resume)
    model = model.to(device).eval()
    # Upstream ERB matrices retain requires_grad=False. BatchNorm stays in eval.
    parameters = [p for p in model.parameters() if p.requires_grad]
    optimizer = torch.optim.AdamW(parameters, lr=lr, weight_decay=1e-5)
    epoch0, step, best, stale = 0, 0, float("inf"), 0
    if resume:
        if checkpoint["pairs_sha256"] != data_hash:
            raise ValueError("Pairs manifest changed since this checkpoint")
        optimizer.load_state_dict(checkpoint["optimizer"])
        for group in optimizer.param_groups:
            group["lr"] = lr
        epoch0, step = checkpoint["epoch"], checkpoint["step"]
        best, stale = checkpoint["best_loss"], checkpoint["stale_epochs"]
        torch.set_rng_state(checkpoint["torch_rng_state"])
        if device == "cuda" and checkpoint.get("cuda_rng_state") is not None:
            torch.cuda.set_rng_state_all(checkpoint["cuda_rng_state"])
    if epoch0 >= epochs:
        raise ValueError("--epochs is the total epoch limit; increase it to continue")
    output.mkdir(parents=True, exist_ok=True)
    rows = {split: [r for r in manifest["scenes"] if r["split"] == split] for split in ("train", "val")}
    if not all(rows.values()):
        raise ValueError("Both train and validation scenes are required")
    config = {"epochs": epochs, "batch": batch, "device": device, "lr": lr, "patience": patience, "seed": seed,
        "pairs_sha256": data_hash, "vendor": verify_vendor(), "status": "experimental_finetune"}
    (output / "config.json").write_text(json.dumps(config, indent=2) + "\n")
    for epoch in range(epoch0 + 1, epochs + 1):
        started = time.monotonic()
        training = list(rows["train"])
        np.random.default_rng(seed + epoch).shuffle(training)
        totals = {}
        for split, selected in (("train", training), ("val", rows["val"])):
            losses = []
            progress = tqdm(_batches(selected, pairs, batch), total=(len(selected)+max(1,batch//3)-1)//max(1,batch//3),
                desc=f"Epoch {epoch}/{epochs} {split}", unit="batch")
            for wave, clean in progress:
                wave, clean = wave.to(device), clean.to(device)
                with torch.set_grad_enabled(split == "train"), torch.backends.cudnn.flags(enabled=False):
                    mixture = spectrum(wave)
                    prediction = torch.view_as_complex(model(torch.view_as_real(mixture).contiguous()).contiguous())
                    loss = spectral_loss(prediction, spectrum(clean), mixture)
                    if not torch.isfinite(loss):
                        raise RuntimeError("Nonfinite loss; no checkpoint exported")
                    if split == "train":
                        optimizer.zero_grad(set_to_none=True)
                        loss.backward()
                        torch.nn.utils.clip_grad_norm_(parameters, 1.)
                        optimizer.step()
                        step += 1
                losses.append((float(loss.detach()), len(wave)))
                progress.set_postfix(loss=f"{losses[-1][0]:.5f}")
            totals[split] = sum(value * count for value, count in losses) / sum(count for _, count in losses)
        improved = totals["val"] < best - 1e-5
        best, stale = (totals["val"], 0) if improved else (best, stale + 1)
        state = {"kind": "quietweft_finetune_v1", "model": model.cpu().state_dict(), "optimizer": optimizer.state_dict(),
            "epoch": epoch, "step": step, "best_loss": best, "stale_epochs": stale,
            "pairs_sha256": data_hash, "config": config, "torch_rng_state": torch.get_rng_state(),
            "cuda_rng_state": torch.cuda.get_rng_state_all() if device == "cuda" else None}
        # Replace atomically: interruption cannot leave a half-written last.pt.
        for name in (["last.pt", "best.pt"] if improved else ["last.pt"]):
            temporary = output / (name + ".tmp")
            torch.save(state, temporary)
            temporary.replace(output / name)
        model.to(device)
        record = {"epoch": epoch, "step": step, "train_loss": totals["train"], "val_loss": totals["val"],
            "best": improved, "seconds": time.monotonic()-started}
        with (output / "history.jsonl").open("a") as handle:
            handle.write(json.dumps(record) + "\n")
        print(json.dumps(record), flush=True)
        if stale >= patience:
            print(f"Early stopping: no validation improvement for {patience} epochs.", flush=True)
            break
    return output / "best.pt"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("prepare", help="Build source-disjoint matched triplets; never downloads audio")
    for name in ("speech", "noise", "output"):
        p.add_argument("--"+name, type=Path, required=True)
    p.add_argument("--scenes", type=int, default=4000)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--seconds", type=float, nargs="+", default=[1., 3., 10.])
    p = sub.add_parser("fit", help="Fine-tune pretrained GTCRN, retaining best/last checkpoints")
    for name in ("pairs", "output"):
        p.add_argument("--"+name, type=Path, required=True)
    p.add_argument("--epochs", type=int, default=100)
    p.add_argument("--batch", type=int, default=12)
    p.add_argument("--device", choices=["auto", "cuda", "cpu"], default="auto")
    p.add_argument("--resume", type=Path)
    p.add_argument("--patience", type=int, default=12)
    p.add_argument("--lr", type=float, default=1e-5)
    p.add_argument("--seed", type=int, default=42)
    p = sub.add_parser("export", help="Export explicit trained checkpoint with streaming/cache parity checks")
    p.add_argument("--checkpoint", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p = sub.add_parser("inspect", help="Inspect a pairs manifest or model manifest")
    p.add_argument("path", type=Path)
    args = vars(parser.parse_args())
    command = args.pop("command")
    torch.set_num_threads(1)
    if command == "prepare":
        result = prepare(**args)
        print(f"Prepared {len(result['scenes'])} matched triplets; 3 examples per scene.")
    elif command == "fit":
        print(f"Best checkpoint: {fit(**args)}")
    elif command == "export":
        from .export import export
        export(**args)
    else:
        path = args["path"]
        if path.is_dir():
            path /= "manifest.json"
        print(json.dumps(json.loads(path.read_text()), indent=2))


if __name__ == "__main__":
    main()
