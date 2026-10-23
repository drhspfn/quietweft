# Third-party notices

Quietweft's own code and brand assets are MIT licensed. Dependencies, pretrained weights and example recordings retain their respective licenses; the root MIT license does not replace them.

| Component | Source | License / notices |
| --- | --- | --- |
| GTCRN architecture, streaming conversion and DNS3 checkpoint | [Rong Xiaobin / GTCRN](https://github.com/Xiaobin-Rong/gtcrn) | MIT; copies in `vendor/GTCRN/LICENSE` and `models/gtcrn-dns3-v1/LICENSE.txt` |
| ONNX Runtime | [Microsoft ONNX Runtime](https://github.com/microsoft/onnxruntime) | MIT and upstream third-party notices in `licenses/` |
| ort Rust bindings | [pykeio/ort](https://github.com/pykeio/ort) | MIT OR Apache-2.0 |
| Tauri | [tauri-apps/tauri](https://github.com/tauri-apps/tauri) | MIT OR Apache-2.0 |
| CPAL | [RustAudio/cpal](https://github.com/RustAudio/cpal) | Apache-2.0 |
| React | [facebook/react](https://github.com/facebook/react) | MIT |
| Lucide icons | [lucide-icons/lucide](https://github.com/lucide-icons/lucide) | ISC |

Resolved dependencies are recorded in the Cargo and npm lockfiles. Bundled dependency license texts are included in `licenses/`. Python training dependencies are installed separately and retain their upstream licenses. No dataset is included in the training package.

Windows packages may carry Microsoft DirectML and Visual C++ runtime redistributables required by the native build. These binaries remain subject to Microsoft's redistribution terms; they are not relicensed under Quietweft's MIT license. Their license notices accompany the Windows package. WebView2 is a separately installed Microsoft runtime.

Illustrative audio/video files have separate attribution and licenses in `docs/media/LICENSE.md`. Their inclusion does not imply endorsement by the original authors. No private microphone recordings are distributed.
