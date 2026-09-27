# Sloppy Paint

A small MS Paint-style drawing program written in Rust.

## v0.1

- Window and drawing canvas
- Pencil
- Eraser
- Color palette
- Brush size from 1 to 64 pixels
- New canvas
- Open PNG
- Save PNG
- Simple brush cursor

## Run

```text
cargo run
```

## Build release

```text
cargo build --release
```

The canvas is currently 1000 × 700 pixels. Images larger than the canvas are cropped when opened; smaller images are placed at the top-left on a white canvas.
