# Binary runtime license sources

- `onnxruntime.txt` and `onnxruntime-third-party.txt`: Microsoft ONNX Runtime v1.31.0, https://github.com/microsoft/onnxruntime/tree/v1.31.0 .
- `directml-1.15.4.txt`: Microsoft.AI.DirectML 1.15.4 redistributable terms, https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4/License . These apply to the DLL, independently of the DirectML sample repository's MIT license.
- `visual-c-runtime-license.docx`: Microsoft Visual C++ Runtime 2015–2022 license, https://visualstudio.microsoft.com/license-terms/vs2022-cruntime/ .
- `MPL-2.0.txt`: canonical Mozilla Public License 2.0, https://www.mozilla.org/MPL/2.0/ . The selectors dependency retains this license; its source repository and exact version are recorded in index.json and Cargo.lock. Corresponding source is available at https://crates.io/crates/selectors/0.38.0 . No modifications to this crate are made by Quietweft.

The Rust/npm inventory and copied notices are in README.md and index.json. Missing license texts in individual published crate archives are explicitly recorded in the index; the SPDX metadata and upstream source coordinates remain available. Native binary bundles include this directory. These upstream terms do not change Quietweft's own MIT license.
