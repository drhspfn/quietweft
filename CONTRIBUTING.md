# Contributing

Open an issue with a reproducible problem or proposed change. For audio defects, include your OS, device sample rate, app version, model ID, routing and a recording only if you have permission to share it. Do not include private conversations or unlicensed datasets.

Keep changes focused and use a short-lived branch. Include a regression test when practical. Run the checks relevant to your change:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --workspace
uv sync --extra test
uv run pytest
cd apps/desktop
npm ci
npm run build:web
```

See [Building](docs/BUILDING.md) for native prerequisites. The desktop and CLI share one runtime; avoid introducing a separate signal-processing implementation. Do not replace the released model in place: add an explicitly versioned candidate with provenance and reproducible evaluation first. A lower training loss alone is not evidence of a better user result.

Contributions are provided under the project's MIT license, while third-party components retain their original terms. Do not commit datasets, local runs, credentials or generated build directories.
