# Orator

Push-to-talk voice-to-text for macOS. Hold Right Option, speak, release to inject transcribed text into the active window.

## What it does

- Listens for Right Option key press (global hotkey)
- Captures microphone audio while held
- Streams audio through sherpa-onnx (Kroko zipformer model) for real-time STT
- Shows a floating overlay with live transcript
- On key release, injects the final text into whatever window is focused
- Supports hotword boosting for technical vocabulary

## Setup

### Prerequisites
- macOS (Apple Silicon or Intel)
- [Nix package manager](https://determinate.systems/nix-installer/)

### Install and run

```bash
# One-liner
nix run github:gudnuf/orator

# Or clone and run locally
git clone https://github.com/gudnuf/orator.git
cd orator
nix run .
```

### macOS permissions

On first run, macOS will prompt for:
1. **Accessibility** (System Settings > Privacy & Security > Accessibility) -- needed for text injection and global hotkey
2. **Microphone** (System Settings > Privacy & Security > Microphone) -- needed for audio capture

Grant both permissions to the Terminal app (or Orator.app if using the launchd agent).

### Development setup

```bash
cd orator
nix develop           # enter dev shell with Rust toolchain + sherpa-onnx
./scripts/download-model.sh  # download model files for local dev
cargo run             # build and run
```

## Usage

1. Focus any text input (editor, terminal, browser, etc.)
2. Hold **Right Option** key
3. Speak naturally
4. Watch the floating overlay for live transcription
5. Release Right Option -- text is injected at the cursor

The overlay shows:
- **Pulsing blue stripe**: listening, waiting for speech
- **Solid blue stripe + text**: actively transcribing
- Fades out when you release the key

## Customizing hotwords

Edit `hotwords.txt` to boost recognition of domain-specific terms. The file uses sherpa-onnx's hotword format (one entry per line, no comments allowed):

```
▁k u b e r n e t e s :2.5
▁n i x p k g s :4.0
```

**Format**: Split the word into individual characters with spaces, prefix with `▁` (Unicode U+2581, the "lower one eighth block"), then `:score`. Higher scores (1.5-4.5) bias the model more strongly toward that word. Do NOT use comment lines -- sherpa-onnx parses every non-empty line as a hotword entry.

When running via `nix run`, set `ORATOR_DATA_DIR` to a directory containing your custom `hotwords.txt`:

```bash
ORATOR_DATA_DIR=~/my-orator-config nix run github:gudnuf/orator
```

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No text appears | Grant Accessibility permission, restart orator |
| No audio captured | Grant Microphone permission, check default input device |
| Model not found | Run `./scripts/download-model.sh` or use `nix run .` (bundles model) |
| Special characters instead of text | This happens if you type while Right Option is held -- wait for release |

## Architecture

Single Rust binary with these modules:
- `hotkey.rs` -- global Right Option key listener (rdev)
- `audio.rs` -- microphone capture (cpal)
- `recognizer.rs` -- streaming STT (sherpa-onnx)
- `inject.rs` -- text injection into active window (enigo)
- `overlay/macos.rs` -- floating HUD overlay (AppKit via objc2)
- `main.rs` -- orchestration loop
