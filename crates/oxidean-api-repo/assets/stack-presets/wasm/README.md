# WebAssembly (Rust)

`cdylib` crate aimed at `wasm32-unknown-unknown` (wasm-pack style stub).

## Getting started

```bash
rustup target add wasm32-unknown-unknown
cargo check --target wasm32-unknown-unknown
# optional:
# wasm-pack build --target web
```

Open `index.html` via a local static server after building with wasm-pack.
