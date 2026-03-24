# Orator

Push-to-talk voice-to-text for macOS. Hold Right Option, speak, release to inject text.

Uses [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) with the Kroko streaming zipformer model for fast, local speech recognition. No cloud services, no API keys.

## Install

```bash
nix run github:gudnuf/orator
```

Requires macOS and [Nix](https://determinate.systems/nix-installer/). On first run, grant Accessibility and Microphone permissions when prompted.

## How it works

1. Hold **Right Option** key
2. Speak -- a floating overlay shows live transcription
3. Release -- text is injected into the active window

## Overlay Styles

Choose a visual style for the floating overlay with `--style <name>`:

```bash
orator --style bifrost      # default
orator --style stormforge
orator --style uru
```

| Style | Description |
|-------|-------------|
| **bifrost** | Dark vibrancy with pulsing electric blue left-edge stripe (default) |
| **stormforge** | Dark HUD with amber glowing dot indicator and semibold text |
| **uru** | Minimal black terminal with blinking underscore cursor |

## Hotwords

Edit `hotwords.txt` to boost recognition of technical terms. One entry per line (no comments):

```
▁k u b e r n e t e s :2.5
▁n i x p k g s :4.0
```

Format: BPE tokens (characters separated by spaces, prefixed with `▁`) then `:boost_score` (1.5-4.5).

Override the default hotwords file:

```bash
ORATOR_DATA_DIR=~/my-config nix run github:gudnuf/orator
```

## Development

```bash
git clone https://github.com/gudnuf/orator.git
cd orator
nix develop                   # Rust toolchain + sherpa-onnx libs
./scripts/download-model.sh   # download STT model (~70MB)
cargo run
```

## License

MIT
