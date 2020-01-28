# Neovide

This is a simple graphical user interface for Neovim. Where possible there are some graphical improvements, but it should act
functionally like the terminal UI.

![Basic Screen Cap](./assets/BasicScreenCap.png)

I've been using this as my daily driver since November 2019. It should be relatively stable, but I'm still working out some kinks 
and ironing out some cross platform issues. In general it should be usable at this point, and if it isn't I consider that a bug and 
appreciate a report in the issues! Any help and ideas are also greatly appreciated.

I'm also very interested in suggestions code quality/style wise when it comes to Rust. I'm pretty new to the language and appreciate 
any critiques that you might have to offer. I won't take all of them, but I promise to consider anything you might have to offer.

## Features

Should be a standard full features Neovim GUI. Beyond that there are some visual niceties:

### Ligatures

Supports ligatures and full HarfBuzz backed font rendering.

![Ligatures](./assets/Ligatures.png)

### Animated Cursor

Cursor animates into position with a smear effect to improve tracking of cursor position.

![Animated Cursor](./assets/AnimatedCursor.gif)

### Emoji Support

Font fallback supports rendering of emoji not contained in the configured font.

![Emoji](./assets/Emoji.png)

#### More to Come

I've got more ideas for simple unobtrusive improvements. More to come.

## Install

Currently there is just a Windows binary under the [project releases](https://github.com/Kethku/neovide/releases). I'm 
hoping to automate and produce Mac and Linux binaries as well, but I haven't gotten there yet.

Installing should be as simple as downloading the binary, making sure `nvim.exe` is on your path, and running it. Everything 
should be self contained.

## Building

Building instructions are somewhat limited at the moment. All the libraries I use are cross platform and should have
support for Windows, Mac, and Linux. The rendering however is Vulkan-based, so driver support for Vulkan will be
necessary. On Windows this should be enabled by default if you have a relatively recent system.

### Windows

1. Install the latest version of Rust. I recommend <https://rustup.rs/>
2. Ensure graphics libraries are up to date.
3. `git clone https://github.com/Kethku/neovide`
4. `cd neovide`
5. `cargo build --release`
6. Copy `./target/release/neovide.exe` to a known location and enjoy.

### Mac

1. Install the latest version of Rust. I recommend <https://rustup.rs/>
2. Install the Vulkan SDK. I'm told `brew cask install apenngrace/vulkan/vulkan-sdk` works, but I can't test locally to find out.
3. `git clone https://github.com/Kethku/neovide`
4. `cd neovide`
5. `cargo build --release`
6. Copy `./target/release/neovide` to a known location and enjoy.

### Linux

Note: Neovide has been compiled for multiple other distros, but the commands may need to be modified slightly to work.

1. Install `bzip2-devel` (or similar, depending on your distro)
2. Install Vulkan drivers. I'm not sure how on Linux. Id appreciate a PR if you know more :)
3. Depending on which libraries are already installed in the system, additonal libraries may need to be installed (Never
   fear, we will do our best to add them here. Make an issue if you find one!)
4. If needed, install [vulkan-tools](https://github.com/LunarG/VulkanTools), etc. Information available in the 
   [vulkan](https://vulkan.lunarg.com/sdk/home) download page.
5. Download the [Vulkan SDK for Linux](https://vulkan.lunarg.com/sdk/home) and extract it in an easily accessible
   location.
6. source /path/to//vulkansdk-linux-x86_64-1.1.130.0/1.1.130.0/setup-env.sh (version can change over time) in the shell 
   that will be used to compile `Neovide`
7. Install the latest version of Rust. I recommend <https://rustup.rs/>
8. `git clone https://github.com/Kethku/neovide`
9. `cd neovide`
10. `cargo build --release`
11. Copy `./target/release/neovide` to a known location and enjoy.

If you see an error complaining about DRI3 settings, links in this issue may help: 
<https://github.com/Kethku/neovide/issues/44#issuecomment-578618052>.

Note: Currently some people seem to be encountering problems with Wayland: <https://github.com/aclysma/skulpin/issues/36>. 
Any help would be appreciated.
