# Neovide
This is a simple graphical user interface for Neovim. Where possible there are some graphical improvements, but it should act
functionally like the terminal UI.

![Basic Screen Cap](./assets/BasicScreenCap.png)

## Features
Should be a standard full features Neovim GUI. Beyond that there are some visual nicities:

### Ligatures

Supports ligatures and full Harbuzz backed font rendering.

![Ligatures](./assets/Ligatures.png)

### Animated Cursor

Cursor animates into position with a smear effect to improve tracking of cursor position.

![Animated Cursor](./assets/AnimatedCursor.gif)

### Emoji Support

Font fallback supports rendering of emoji not contained in the configured font.

![Emoji](./assets/Emoji.png)

#### More to Come

I've got more ideas for simple unobtrusive improvements. More to come.

## Building

Building instructions are somewhat limited at the moment. All the libraries I use are cross platform and should have
support for Windows Mac and Linux. The rendering however is Vulcan based, so driver support for vulcan will be
necessary. On Windows this should be enabled by default if you have a relatively recent system.

Building requires a modern copy of Rust and should be as simple as running `cargo build --release` and running the
resulting binary in `targets/release`.

Better support and prebuilt binaries for non windows platforms is planned.
