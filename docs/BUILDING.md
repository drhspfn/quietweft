# Build from source

Install Rust stable, Python 3.11+, Node.js 20+ and npm. The model is pinned in `models/gtcrn-dns3-v1` and included in packages. Builds do not download training datasets or virtual audio drivers. Cargo may download ONNX Runtime binaries while compiling; npm/Cargo download dependencies on first use.

Windows x64: install Visual Studio 2022 Build Tools with **Desktop development with C++** and the Windows SDK. WebView2 is required for the desktop app (included with current Windows 10/11). The script initializes the MSVC environment automatically.

Linux (Ubuntu/Debian):

```sh
sudo apt-get install build-essential pkg-config libasound2-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

macOS: install Xcode command-line tools (`xcode-select --install`). The desktop binary uses system WebKit.

From the repository root:

```sh
python scripts/build.py --platform windows --target all
python scripts/build.py --platform linux --target cli
python scripts/build.py --platform macos --target desktop
```

Choose your **host** OS; this script does not pretend to cross-compile native audio/WebView dependencies. CI runs on each native OS. Output is a portable folder plus ZIP in `dist`. Existing output folders are never overwritten; select `--output another-folder` for a subsequent build. `--target all` produces separate desktop and CLI/embedding archives. Desktop packaging is portable, not an installer; macOS signing/notarization, Windows ARM64, and Linux distribution-specific packaging are not provided.

Development checks:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --workspace
cargo run --manifest-path rust/Cargo.toml -p quietweft-cli -- inspect
cd apps/desktop
npm ci
npm run desktop:dev
```

Optional `CARGO_TARGET_DIR` shares a compiled cache. ONNX Runtime and MSVC redistributables retain their upstream licenses. Unsigned desktop binaries may trigger OS download warnings; published artifacts are accompanied by hashes, not a code-signing identity.

