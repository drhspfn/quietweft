# Models

Quietweft 1.4.4 ships one pinned model, `gtcrn-dns3-v1`. The same ONNX bytes are
included in the repository, the model-only release archive, the desktop package
and the CLI package. The application does not download or switch models online.

The registry is [`models/manifest.json`](../models/manifest.json). Each model
directory contains `model.onnx`, a runtime manifest, provenance and its license.
SHA256:

```text
d2ced5e4977085ce91b8147a18de1189c972789bff257ec9c83ae19eed9936bb
```

This is the official DNS3-pretrained
[GTCRN](https://github.com/Xiaobin-Rong/gtcrn), converted to explicit-state
streaming ONNX. It is not a model trained from scratch by Quietweft. Upstream
copyright and MIT attribution are retained. The application version and model
identifier are independent; a UI release does not imply new model weights.

Inference uses 16 kHz mono audio, 512-sample analysis frames and a 256-sample
analysis hop. The runtime accepts 128-sample transport blocks and contributes
384 samples / 24 ms of algorithmic delay. Device and host buffering add latency.

The interface exposes an On/Off switch. Mixing original audio into the result
is not a learned suppression-strength parameter. Off retains the same delay,
and model state continues so enabling processing does not cold-start it.

Training and export produce a separate candidate directory. They never replace
the shipped model automatically. A candidate needs evaluation before its
registry entry, checksum and release package should be updated. See
[Training](TRAINING.md) and [Measurements](MEASUREMENTS.md).
