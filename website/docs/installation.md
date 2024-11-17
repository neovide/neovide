# Installation

**Note**: Neovide requires neovim version `0.10` _or greater_. See previous releases such as `0.5.0`
if your distro is too slow with updating or you need to rely on older neovim versions.

Building instructions are somewhat limited at the moment. All the libraries Neovide uses are cross
platform and should have support for Windows, Mac, and Linux. The rendering is based on OpenGL, so a
good GPU driver will be necessary, the default drivers provided by virtual machines might not be
enough. On Windows this should be enabled by default if you have a relatively recent system.

## Binaries

Installing should be as simple as downloading the binary, making sure the `nvim` executable with
version 0.10 or greater is on your `PATH` environment variable, and running it. Everything should be
self contained.

The binaries are to be found on
[the release page](https://github.com/neovide/neovide/releases/latest).

## Windows

### Scoop

[Scoop](https://scoop.sh/) has Neovide in the `extras` bucket. Ensure you have the `extras` bucket,
and install:

```sh
$ scoop bucket list
main
extras

$ scoop install neovide
```

### Windows Source

1. Install the latest version of Rust. I recommend <https://rustup.rs/>

2. Install CMake. I use chocolatey:
   `choco install cmake --installargs '"ADD_CMAKE_TO_PATH=System"' -y`

3. Install LLVM. I use chocolatey: `choco install llvm -y`

4. Ensure graphics libraries are up to date.

5. Build and install Neovide:

   ```sh
   cargo install --git https://github.com/neovide/neovide.git
   ```

   The resulting binary can be found inside of `~/.cargo/bin` afterwards (99% of the time).

## Mac

### Homebrew

Neovide is available as Cask in [Homebrew](https://brew.sh). It can be installed from the command
line:

```sh
brew install --cask neovide
```

Neovide registers launched shells taking the user's preferred shell into account.

If you are encountering issues with Neovide not being found by your shell, you can try to add the
`brew` binary path to your `PATH` environment variable:

```sh
sudo launchctl config user path "$(brew --prefix)/bin:${PATH}"
```

For more information, see the Homebrew [FAQ](https://docs.brew.sh/FAQ#my-mac-apps-dont-find-homebrew-utilities).

### Mac Source

1. Install the latest version of Rust. Using homebrew: `brew install rustup-init`

2. Configure rust by running `rustup-init`

3. Install CMake. Using homebrew: `brew install cmake`

4. `git clone https://github.com/neovide/neovide`

5. `cd neovide`

6. `cargo install --path .`

   The resulting binary is to be found under `~/.cargo/bin`. In case you want a nice application
   bundle:

7. `GENERATE_BUNDLE_APP=true GENERATE_DMG=true ./macos-builder/run`

8. `open ./target/release/bundle/osx/Neovide.dmg`

## Linux

### Arch Linux

Stable releases are
[packaged in the extra repository](https://archlinux.org/packages/extra/x86_64/neovide).

```sh
pacman -S neovide
```

If you want to run Neovide on X11, you'll also need `libxkbcommon-x11`.

```sh
pacman -S libxkbcommon-x11
```

To run a development version you can build from
[the VCS package in the AUR](https://aur.archlinux.org/packages/neovide-git). This can be built and
installed using an AUR helper or
[by hand in the usual way](https://wiki.archlinux.org/title/Arch_User_Repository#Installing_and_upgrading_packages).
To build from a non-default branch you can edit the PKGBUILD and add `#branch-name` to the end of
the source URL.

### Nix

Stable releases are packaged in nixpkgs in the `neovide` package, there's no flake. As such, if you
just want to try it out in a transient shell, you can use this command.

**Note**: On non-NixOS systems, chances are you'll need to use
[nixGL](https://github.com/nix-community/nixGL) as wrapper for neovide.

```sh
nix-shell -p neovide
```

#### NixOS

Just add `neovide` from nixpkgs to your `environment.systemPackages` in `configuration.nix`.

```nix
environment.systemPackages = with pkgs; [neovide];
```

### Linux Source

1. Install necessary dependencies (adjust for your preferred package manager, probably most of this
   stuff is already installed, just try building and see)

   - Ubuntu/Debian

     ```sh
     sudo apt install -y curl \
         gnupg ca-certificates git \
         gcc-multilib g++-multilib cmake libssl-dev pkg-config \
         libfreetype6-dev libasound2-dev libexpat1-dev libxcb-composite0-dev \
         libbz2-dev libsndio-dev freeglut3-dev libxmu-dev libxi-dev libfontconfig1-dev \
         libxcursor-dev
     ```

   - Fedora

     ```sh
     sudo dnf install fontconfig-devel freetype-devel libX11-xcb libX11-devel libstdc++-static libstdc++-devel
     sudo dnf groupinstall "Development Tools" "Development Libraries"
     ```

   - Arch

     Do note that an [AUR package](https://aur.archlinux.org/packages/neovide-git) already exists.

     ```sh
     sudo pacman -S base-devel fontconfig freetype2 libglvnd sndio cmake \
         git gtk3 python sdl2 vulkan-intel libxkbcommon-x11
     ```

2. Install Rust

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf "https://sh.rustup.rs" | sh
   ```

3. Fetch and build

   ```sh
   cargo install --git https://github.com/neovide/neovide
   ```

   The resulting binary can be found inside of `~/.cargo/bin` afterwards, you might want to add this
   to your `PATH` environment variable.
