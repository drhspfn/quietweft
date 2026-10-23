# Embedding

`quietweft-runtime` exposes a Rust API and a C ABI. The runtime uses one CPU ONNX session per handle and carries recurrent/STFT state between calls. Each independent stream needs its own handle. No microphone capture, device routing, or virtual cable is required when embedding.

Rust: depend on `quietweft-runtime` from `rust/runtime`, load `Runtime::load(model_dir)`, then call `process(&[f32;128])`. `set_enabled(bool)` preserves state and crossfades over the established 8 ms transport steps. `process_recording` is a one-recording helper for a fresh handle and compensates the 384-sample delay.

C: include `include/quietweft.h` and link against `quietweft_runtime` (`.lib` plus `.dll` on Windows; `.so` on Linux; `.dylib` on macOS). `examples/c/embed.c` demonstrates raw float streaming. Keep the packaged model directory and native dependencies available. The C ABI version is 1. All sample pointers must reference 128 valid float elements; never call a handle concurrently. The runtime checks model SHA256 before initializing ONNX.

```sh
cc examples/c/embed.c -Iinclude -L/path/to/package -lquietweft_runtime -Wl,-rpath,/path/to/package -o embed
./embed /path/to/package/models/gtcrn-dns3-v1 < input.f32 > output.f32
```

On Windows use a Developer Command Prompt:

```bat
cl examples\c\embed.c /Iinclude /link /LIBPATH:PACKAGE_DIRECTORY quietweft_runtime.dll.lib
```

The C sample uses binary IEEE-754 float streams and sets Windows CRT standard streams to binary mode. For durable integration, prefer host audio buffers directly.

Sample rate is fixed at 16000 Hz mono, with 128-sample calls. The model runs STFT 512/256; the scheduler and reconstruction introduce 384 samples (24 ms) of delay. Capture, resampling, queue, playback, and OS buffers add latency. The shared `quietweft-audio` crate offers microphone/device handling for desktop/CLI; its linear streaming resampler is intended for voice monitoring, not audio mastering.

Application source is MIT. The bundled GTCRN source/model and ONNX Runtime retain their own license notices; see THIRD_PARTY_NOTICES.md. Model performance and distribution compatibility should be assessed for your deployment.
