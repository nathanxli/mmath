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

## Options

Everything is configurable in the start menu. Flags just preset a toggle:

| Flag | Effect |
| --- | --- |
| `-m`, `--mult-choice` | Multiple choice: pick from a 2x2 grid by click or keys 1-4 |
| `-s` | Small text (large text is on by default) |

Voice input (offline speech recognition via Vosk) lives on the `voice`
branch; it is not part of the default install.

## Uninstall

```bash
cargo uninstall mmath          # if installed with `cargo install`
sudo rm /usr/local/bin/mmath   # if you symlinked it instead
```

Then delete the clone. Build output (`target/`) lives inside it.

