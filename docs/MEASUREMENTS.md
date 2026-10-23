# Processing measurements

These numbers describe CPU processing cost and the runtime's algorithmic delay. They do not evaluate noise-removal quality and are not microphone-to-headphones round-trip measurements.

## Windows reference run

Measured on 2026-10-23 using the release-mode Quietweft 1.4.4 CLI, AMD Ryzen 5 3600 (6 cores / 12 logical processors), Windows 11 Pro 10.0.28000, CPU execution and one ONNX Runtime intra-op thread. The runtime reports ONNX Runtime `rel-1.31.0`, commit `402979a`. The pinned model's hash is in [the registry](../models/manifest.json).

Command, run inside the extracted CLI package:

```powershell
./quietweft.exe benchmark
```

| Measurement | Observed |
| --- | ---: |
| Generated input duration | 10 seconds |
| Mean processing time per 128-sample transport block | 468.324 µs |
| Maximum observed transport-block processing time | 2297.5 µs |
| Processing real-time factor | 0.05854 |
| Algorithmic delay | 384 samples / 24 ms at 16 kHz |

This is a single run under the machine's current workload, not a universal bound. The generated input is a sine signal. `benchmark input.wav` measures a particular recording instead. Model initialization and file decoding are outside the processing timer. Initial inference is included; the final zero blocks flush the recording. Every second transport block runs a model hop, so transport timings include both inexpensive scheduling blocks and inference blocks. There is no percentile estimate in this measurement.

The real-time factor divides accumulated processing time by processed audio duration; lower than 1 means that this run processed faster than real time. It is not total process CPU utilization or a guarantee against audio dropouts.

## Latency boundaries

The 24 ms algorithmic delay includes the analysis/overlap-add path and transport scheduling. Live capture buffers, output buffers, sample-rate conversion, OS scheduling, device latency and a virtual cable add further delay. Actual end-to-end latency needs a physical or loopback measurement on the target audio setup; none is claimed here.

File processing compensates the 384-sample delay and preserves the exact input length. Raw `pipe` and the C/Rust streaming API retain the delay, as documented in [Embedding](EMBEDDING.md).

## Correctness checks

The public runtime was compared with the frozen working private runtime using the same model and a deterministic 16,384-sample sine/noise input: outputs were bit-identical on the reference Windows machine. Native tests cover the pinned model, partial final blocks, delayed bypass, invalid samples and manifest/hash rejection. Python tests execute a small real training run, checkpoint resume and streaming ONNX parity. These checks establish implementation behavior, not perceptual quality.

The extracted Windows desktop package was also launched and checked with a USB microphone and VB-CABLE output: device discovery, starting capture, live meters, switching suppression off while running and stopping capture worked. The headphone route and idle state were checked visually. No microphone recording was retained. This smoke check is not a listening-quality evaluation or latency measurement.
