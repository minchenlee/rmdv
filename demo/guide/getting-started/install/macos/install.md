# Install rmdv on macOS

[← back to README](../../../../README.md)

## Requirements

- [x] macOS 13 or newer
- [x] Apple Silicon **or** Intel
- [ ] Rosetta (only if running the x86_64 build on Apple Silicon)

## Download

Grab the latest `.dmg` from [Releases](https://github.com/minchenlee/rmdv/releases/latest), or build from source:

```bash
# clone + build the release binary (always --release; debug is too slow)
git clone https://github.com/minchenlee/rmdv.git
cd rmdv
cargo build --release
./target/release/rmdv --help
```

## First launch

Gatekeeper may quarantine an unsigned build. Clear it:

```bash
xattr -dr com.apple.quarantine /Applications/rmdv.app
open /Applications/rmdv.app
```

## Open a folder

```bash
# open this very demo vault
rmdv /path/to/rmdv/demo
```

Next: [Quickstart →](../../first-steps/quickstart.md)
