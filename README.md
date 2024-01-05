# bloop.rs

Opinionated MIDI looper written in Rust

## Planned features

- [x] Multiple channels
- [ ] Shortcut customization
- [ ] Ad-hoc tempo adjustment
- [ ] Quantization
- [ ] MIDI file export

## Building

### System requirements

- **bloop.rs requires the latest version of the Rust compiler.**
- **bloop.rs currently runs on Linux and macOS, but not Windows.** This is because [midir](https://github.com/Boddlnagg/midir) does not support creating virtual MIDI outputs on Windows. This may be remedied in the future by using an [external program](https://www.tobias-erichsen.de/software/loopmidi.html) to create a virtual MIDI output.

## Building on Linux or macOS

1. Download/install Cargo.
2. Clone this project and build/run:

```sh
git clone https://github.com/HactarCE/blooprs
cd blooprs
cargo run --release
```

The first build may take ~10 minutes or more. Remove `--release` to disable optimizations, which makes building faster but bloop.rs may run slower.

## Usage

TODO: write this
