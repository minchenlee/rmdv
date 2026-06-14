# Images

[← back to README](../../../README.md)

rmdv renders inline images — local relative paths and remote URLs — and a click opens the **zoom modal**.

## Local image

Resolved against this file's folder:

![rmdv icon](icon.png)

## Remote SVG

![iced logo](https://raw.githubusercontent.com/iced-rs/iced/master/docs/logo.svg)

## Remote PNG

![rust logo](https://www.rust-lang.org/static/images/rust-logo-blk.svg)

## Broken path (graceful)

A missing image degrades cleanly instead of breaking layout:

![missing](does/not/exist.png)

> Click any image to open the zoom modal; scroll to zoom, drag to pan, Esc to close.
