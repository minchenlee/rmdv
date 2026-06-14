# Install rmdv on Linux

[← back to README](../../../../README.md)

## AppImage (no install)

```bash
curl -L -o rmdv.AppImage \
  https://github.com/minchenlee/rmdv/releases/latest/download/rmdv-x86_64.AppImage
chmod +x rmdv.AppImage
./rmdv.AppImage demo/
```

## From source

```bash
# Debian/Ubuntu build deps
sudo apt install -y build-essential pkg-config libfontconfig1-dev
cargo build --release
```

## Checklist

- [x] glibc-based distro (Ubuntu, Fedora, Arch)
- [x] Wayland or X11
- [ ] musl static build (not provided yet)

> Tip: on Wayland, fractional scaling is handled automatically.

Next: [Quickstart →](../../first-steps/quickstart.md)
