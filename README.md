# Neovide [![Gitter](https://badges.gitter.im/neovide/community.svg)](https://gitter.im/neovide/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge) [![Discussions](https://img.shields.io/badge/GitHub-Discussions-green?logo=github)](https://github.com/Kethku/neovide/discussions)[![Get it from the Snap Store](https://snapcraft.io/static/images/badges/en/snap-store-white.svg)](https://snapcraft.io/neovide)


This is a simple graphical user interface for Neovim. Where possible there are some graphical improvements, but it should act
functionally like the terminal UI.

![Basic Screen Cap](./assets/BasicScreenCap.png)

I've been using this as my daily driver since November 2019. It should be relatively stable, but I'm still working out some kinks
and ironing out some cross platform issues. In general it should be usable at this point, and if it isn't I consider that a bug and
appreciate a report in the issues! Any help and ideas are also greatly appreciated.

I'm also very interested in suggestions code quality/style wise when it comes to Rust. I'm pretty new to the language and appreciate
any critiques that you might have to offer. I won't take all of them, but I promise to consider anything you might have to offer.

 [![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/G2G040CA8)

## Features

Should be a standard full features Neovim GUI. Beyond that there are some visual niceties:

### Ligatures

Supports ligatures and full [HarfBuzz](https://www.freedesktop.org/wiki/Software/HarfBuzz/) backed font shaping.

![Ligatures](./assets/Ligatures.png)

### Animated Cursor

Cursor animates into position with a smear effect to improve tracking of cursor position.

![Animated Cursor](./assets/AnimatedCursor.gif)

### Smooth Scrolling

Scroll operations on buffers in neovim will be animated smoothly pixel wise rather than line by line at a time. Note, multigrid must be
enabled for this to work.
https://github.com/Kethku/neovide/wiki/Configuration#multiGrid

![Smooth Scrolling](./assets/SmoothScrolling.gif)

### Animated Windows

Windows animate into position when they are moved making it easier to see how layout changes happen. Note, multigrid must be enabled for 
this to work.
https://github.com/Kethku/neovide/wiki/Configuration#multiGrid

![Animated Windows](./assets/AnimatedWindows.gif)

### Blurred Floating Windows

The backgrounds of floating windows are blurred improving the visual separation between foreground and background from
built in window transparency. Note, multigrid must be enabled for this to work.
https://github.com/Kethku/neovide/wiki/Configuration#multiGrid

![Blurred Floating Windows](./assets/BlurredFloatingWindows.png)

### Emoji Support

Font fallback supports rendering of emoji not contained in the configured font.

![Emoji](./assets/Emoji.png)

### WSL Support

Neovide supports displaying a full gui window from inside wsl via the `--wsl` command argument. Communication is passed via standard io into the wsl copy of neovim providing identical experience similar to visual studio code's remote editing https://code.visualstudio.com/docs/remote/remote-overview.

### Remote TCP Support

Neovide supports connecting to a remote instance of Neovim over a TCP socket via the `--remote-tcp` command argument. This would allow you to run Neovim on a remote machine and use the GUI on your local machine, connecting over the network.

Launch Neovim as a TCP server (on port 6666) by running:

```sh
nvim --headless --listen localhost:6666
```

And then connect to it using:

```sh
/path/to/neovide --remote-tcp=localhost:6666
```

By specifying to listen on localhost, you only allow connections from your local computer. If you are actually doing this over a network you will want to use SSH port forwarding for security, and then connect as before.

```sh
ssh -L 6666:localhost:6666 ip.of.other.machine nvim --headless --listen localhost:6666
```

### Some Nonsense ;)

```
let g:neovide_cursor_vfx_mode = "railgun"
```

![Railgun](./assets/Railgun.gif)

### More to Come

I've got more ideas for simple unobtrusive improvements. More to come.

## Configuration

Configuration is done almost completely via global neovide variables in your vim config and can be manipulated live at runtime. Details can be found [here](https://github.com/Kethku/neovide/wiki/Configuration).

## Install

Relatively recent binaries can be found in the [project releases](https://github.com/Kethku/neovide/releases). But if you want the latest and greatest you should clone it and build yourself.

Installing should be as simple as downloading the binary, making sure `nvim.exe` with version 0.4 or greater is on your path, and running it. Everything should be self contained.

## Building

Building instructions are somewhat limited at the moment. All the libraries I use are cross platform and should have
support for Windows, Mac, and Linux. The rendering however is based on opengl, so a good gpu driver will be
necessary. On Windows this should be enabled by default if you have a relatively recent system.

Note: Neovide requires neovim version 0.4 or greater.

### Windows

1. Install the latest version of Rust. I recommend <https://rustup.rs/>
2. Install CMake. I use chocolatey: `choco install cmake --installargs '"ADD_CMAKE_TO_PATH=System"' -y`
3. Install LLVM. I use chocolatey: `choco install llvm -y`
4. Ensure graphics libraries are up to date.
5. `git clone https://github.com/Kethku/neovide`
6. `cd neovide`
7. `cargo build --release`
8. Copy `./target/release/neovide.exe` to a known location and enjoy.

### Mac

1. Install the latest version of Rust. I recommend <https://rustup.rs/>
2. Install CMake. Using homebrew: `brew install cmake`
4. `git clone https://github.com/Kethku/neovide`
5. `cd neovide`
6. `cargo build --release`
7. Copy `./target/release/neovide` to a known location and enjoy.

### Linux

#### Arch Linux

There is an [AUR package for neovide](https://aur.archlinux.org/packages/neovide-git/).

```sh
git clone https://aur.archlinux.org/neovide-git.git
cd neovide
makepkg -si
```
Note: Neovide requires that a font be set in `init.vim` otherwise errors might be encountered.
See [#527](https://github.com/Kethku/neovide/issues/527)

This can be fixed by adding `set guifont=Your\ Font\ Name:h15` in init.vim file.

#### Debian/Ubuntu

Note: Neovide has been successfully built on other distros but this reportedly works on ubuntu.

1. Install necessary dependencies

    ```sh
    sudo apt-get install -y curl \
        gnupg ca-certificates git \
        gcc-multilib g++-multilib cmake libssl-dev pkg-config \
        libfreetype6-dev libasound2-dev libexpat1-dev libxcb-composite0-dev \
        libbz2-dev libsndio-dev freeglut3-dev libxmu-dev libxi-dev
    ```

2. Install Rust

    `curl --proto '=https' --tlsv1.2 -sSf "https://sh.rustup.rs" | sh`

3. Clone the repository

    `git clone "https://github.com/Kethku/neovide"`

4. Build

    `cd neovide && ~/.cargo/bin/cargo build --release`

5. Copy `./target/release/neovide` to a known location and enjoy.

If you see an error complaining about DRI3 settings, links in this issue may help:
<https://github.com/Kethku/neovide/issues/44#issuecomment-578618052>.

Note: If you run into libsndio errors, try building without default features which will disable static linking of the SDL
library.
