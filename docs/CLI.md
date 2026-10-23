# Command line

The CLI package includes the pinned ONNX model. No Python or virtual audio cable is needed.

```powershell
./quietweft.exe inspect
./quietweft.exe devices
./quietweft.exe process input.wav --output cleaned.wav
./quietweft.exe benchmark input.wav
./quietweft.exe monitor --input 'DEVICE_ID_FROM_LIST' --output 'DEVICE_ID_FROM_LIST'
```

Use headphones for `monitor` to avoid acoustic feedback. Press Enter to stop. A regular microphone and headphones are sufficient. Sending the result into another application's microphone requires a virtual audio endpoint or integration with that application; Quietweft does not install a driver.

`process` accepts mono 16 kHz integer or floating-point WAV and writes a 32-bit floating-point WAV with the exact input length and compensated algorithmic delay. Existing files are not overwritten. Convert other formats first:

```sh
ffmpeg -i recording.mp3 -ar 16000 -ac 1 input.wav
quietweft process input.wav --output cleaned.wav
```

`pipe` accepts little-endian f32 mono 16 kHz on stdin and produces the same stream format on stdout. Input must contain complete 128-sample blocks. It retains the 384-sample streaming delay, does not trim or flush it, and keeps diagnostics on stderr. Connect binary streams using a binary-safe process API; Windows PowerShell 5 pipelines can corrupt binary data.

Every command except `devices` verifies the SHA256 in the model manifest. Override the packaged model directory with `--model PATH`. `benchmark` without an input uses 10 seconds of generated signal and reports CPU processing timing, not noise-removal quality or hardware round-trip latency.
