# mmath

A lightweight TUI-based speed drill for mental arithmetic.

Inspired by [ttyper](https://github.com/max-niederman/ttyper).


## Requirements

- Rust toolchain (stable): <https://rustup.rs>
- A terminal that supports ANSI/TUI output

## Install

Voice input is off by default, so the base install needs no downloads.

### Option 1: Install from a local clone (recommended)

```bash
git clone <REPO_URL>
cd mmath
cargo install --path .
```

If `~/.cargo/bin` is in your `PATH` (default for most Rust setups), you can run:

```bash
mmath
```

### Option 2: Build + symlink manually

```bash
git clone <REPO_URL>
cd mmath
cargo build --release
ln -s "$(pwd)/target/release/mmath" /usr/local/bin/mmath
```

Then run from anywhere:

```bash
mmath
```

## Run in development

```bash
cargo run
```

## Options

Everything is configurable in the start menu. Flags just preset a toggle:

| Flag | Effect |
| --- | --- |
| `-v`, `--voice` | Answer by speaking (needs a `--features voice` build, see below) |
| `-m`, `--mult-choice` | Multiple choice: pick from a 2x2 grid by click or keys 1-4 |
| `-s` | Small text (large text is on by default) |

Voice and multiple choice are mutually exclusive; `-m` wins if both are given.

## Voice input

Answers can be spoken instead of typed, recognized fully offline with
[Vosk](https://alphacephei.com/vosk/). It is opt-in: it pulls in a native
library and a speech model (~45 MB into `lib/` and `models/`).

```bash
scripts/fetch-voice-assets.sh
cargo install --path . --features voice   # or: cargo run --features voice
```

`voice` is compile-time, so you pass it only when building. The installed
`mmath` keeps voice until you rebuild, so run it with no flags and toggle
voice in the menu (or with `-v`). A later `cargo install --path .` without
`--features voice` silently replaces the binary with one that has none, and
the "Voice input" menu row goes grey. The binary loads `libvosk` from this
clone's `lib/` by absolute path, so keep the project directory in place.

Both "one hundred twenty three" and digit-by-digit "one two three" work.
Keyboard input stays active alongside voice. Pause briefly between answers so
the recognizer can segment utterances. macOS asks for microphone permission on
first use.

Each answer's latency (end of speech to answer registered) is shown in the
header and on the results page, with an adjusted solve time that subtracts it.

The model is loaded from `models/` (first directory starting with
`vosk-model`), or from `MMATH_VOSK_MODEL` if set. If you installed with
`cargo install`, set `MMATH_VOSK_MODEL` or run from the project directory.

## Uninstall

```bash
cargo uninstall mmath          # if installed with `cargo install`
sudo rm /usr/local/bin/mmath   # if you symlinked it instead
```

Then delete the clone. Build output (`target/`) and the voice assets (`lib/`,
`models/`) all live inside it. If you used voice on macOS, revoke the leftover
microphone grant under System Settings > Privacy & Security > Microphone.

