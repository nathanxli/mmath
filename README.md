# mmath

A lightweight TUI-based speed drill for mental arithmetic.

Inspired by [ttyper](https://github.com/max-niederman/ttyper).


## Requirements

- Rust toolchain (stable): <https://rustup.rs>
- A terminal that supports ANSI/TUI output

## Install


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

## Voice input

Answers can be spoken instead of typed. Recognition runs fully offline
([Vosk](https://alphacephei.com/vosk/)) with a grammar restricted to number
words, matching on streaming partial results so an answer registers within
~100ms of you finishing the number.

### Setup

Fetch the native library and speech model (one-time, ~45 MB into `lib/` and
`models/`):

```bash
scripts/fetch-voice-assets.sh
cargo build
```

macOS will ask for microphone permission on first use.

### Usage

Toggle "Voice input" in the start menu, or pre-enable it from the CLI:

```bash
mmath -v            # or --voice
mmath --voice-check # voice on + show per-answer latency (speech start -> answer typed)
```

Both spoken styles work: "one hundred twenty three" and digit-by-digit
"one two three". Keyboard input stays active alongside voice. Pause briefly
between answers so the recognizer can segment utterances.

Notes:

- The recognizer looks for the model in `models/` (first directory starting
  with `vosk-model`), or wherever `MMATH_VOSK_MODEL` points. If you installed
  the binary with `cargo install`, set `MMATH_VOSK_MODEL` or run from the
  project directory.
- Latency plumbing: partial results are matched while you speak (waiting for
  Vosk's final result would add 300-500ms of endpoint silence), and the game
  loop polls at 15ms when voice is on.

