# Command Line Reference

Neovide supports a few command line arguments for effecting things which couldn't be set using
normal vim variables.

`$` in front of a word refers to it being an "environment variable" which is checked for, some
settings only require it to be set in some way, some settings also use the contents.

Note: On macOS, it's not easy to specify command line arguments when launching Apps, you can use
[Neovide Config File](config-file.md) or `launchctl setenv NEOVIDE_FRAME transparent` to
apply those setting.

## Information

### Version

```sh
--version or -V
```

Prints the current version of neovide.

### Help

```sh
--help or -h
```

Prints details about neovide. This will be a help page eventually.

## Functionality

### Frame

```sh
--frame or $NEOVIDE_FRAME
```

Can be set to:

- `full`: The default, all decorations.
- `none`: No decorations at all. NOTE: Window cannot be moved nor resized after this.
- (macOS only) `transparent`: Transparent decorations including a transparent bar.
- (macOS only) `buttonless`: All decorations, but without quit, minimize or fullscreen buttons.

### Window Size

```sh
--size=<width>x<height>
```

Sets the initial neovide window size in pixels.

Can not be used together with `--maximized`, or `--grid`.

### Maximized

```sh
--maximized or $NEOVIDE_MAXIMIZED
```

Maximize the window on startup, while still having decorations and the status bar of your OS
visible.

This is not the same as `g:neovide_fullscreen`, which runs Neovide in "exclusive fullscreen",
covering up the entire screen.

Can not be used together with `--size`, or `--grid`.

### Grid Size

```sh
--grid [<columns>x<lines>]

```

**Available since 0.12.0.**

Sets the initial grid size of the window. If no value is given, it defaults to
columns/lines from `init.vim/lua`, see
[columns](https://neovim.io/doc/user/options.html#'columns') and
[lines](https://neovim.io/doc/user/options.html#'lines').

If the `--grid` argument is not set then the grid size is inferred from the
window size.

Note: After the initial size has been determined and `init.vim/lua` processed,
you can set [columns](https://neovim.io/doc/user/options.html#'columns') and
[lines](https://neovim.io/doc/user/options.html#'lines') inside neovim
regardless of the command line arguments used. This has to be done before any
redraws are made, so it's recommended to put it at the start of the
`init.vim/lua` along with `guifont` and other related settings that can affect
the geometry.

Can not be used together with `--size`, or `--maximized`.

### Log File

```sh
--log
```

Enables the log file for debugging purposes. This will write a file next to the executable
containing trace events which may help debug an issue.

### Multigrid

```sh
--no-multigrid or $NEOVIDE_NO_MULTIGRID
```

This disables neovim's multigrid functionality which will also disable floating window blurred
backgrounds, smooth scrolling, and window animations. This can solve some issues where neovide
acts differently from terminal neovim.

### Fork

```sh
--fork or $NEOVIDE_FORK=0|1
```

Detach from the terminal instead of waiting for the Neovide process to
terminate. This parameter has no effect when launching from a GUI.

### No Idle

```sh
--no-idle or $NEOVIDE_IDLE=0|1
```

With idle `on` (default), neovide won't render new frames when nothing is happening.

With idle `off` (e.g. with `--no-idle` flag), neovide will constantly render new frames,
even when nothing changed. This takes more power and CPU time, but can possibly help
with frame timing issues.

### Mouse Cursor Icon

```sh
--mouse-cursor-icon or $NEOVIDE_MOUSE_CURSOR_ICON="arrow|i-beam"
```

**Available since 0.14.**

This sets the mouse cursor icon to be used in the window.

TLDR; Neovim has not yet implemented the
['mouseshape'](https://github.com/neovim/neovim/issues/21458) feature, meaning that
the cursor will not be reactive respecting the context of any Neovim element such as tabs,
buttons and dividers. For that reason, the Arrow cursor has been taken as the default due
to its generalistic purpose.

### Title (macOS Only)

```sh
--title-hidden or $NEOVIDE_TITLE_HIDDEN
```

**Available since 0.12.2.**

This sets the window title to be hidden on macOS.

### sRGB

```sh
--no-srgb, --srgb or $NEOVIDE_SRGB=0|1
```

Request sRGB support on the window. The command line parameter takes priority
over the environment variable.

On Windows, Neovide does not actually render with sRGB, but it's still enabled
by default to work around
[neovim/neovim/issues/907](https://github.com/neovim/neovim/issues/907).

On macOS, this option works as expected to switch sRGB color space. The
default is `--no-srgb` to keep the behavior of previous versions. If you want
to enable srgb, please use `--srgb`.

Other platforms should not need it, but if you encounter either startup crashes
or wrong colors, you can try to swap the option.

Notes on macOS: Traditional terminals do not use sRGB by default. This is how
most terminals on Windows and Linux do. Neovide follows this rule. However,
Terminal of macOS changes the default to sRGB. Other terminal emulators, like
Alacritty, Kitty, may follow Apple and use sRGB. Some may offer no function
to switch it off currently. So you might get different color of the same value
in Neovide surprisingly. Please read
[neovide/neovide/issues/1102](https://github.com/neovide/neovide/issues/1102)
for more details.

### Tabs

```sh
--no-tabs, --tabs or $NEOVIDE_TABS=0|1
```

By default, Neovide opens files given directly to Neovide (not NeoVim through `--`!) in multiple
tabs to avoid confusing new users. `--no-tabs` disables this behavior.

Note: Even if files are opened in tabs, they're buffers anyways. It's just about them being visible
or not.

### OpenGL Renderer

```sh
--opengl or $NEOVIDE_OPENGL=1
```

By default, Neovide uses D3D on Windows and Metal on macOS as renderer. You
can use `--opengl` to force OpenGL when you meet some problems of D3D/Metal.

### No VSync

```sh
--no-vsync, --vsync or $NEOVIDE_VSYNC=0|1
```

**Available since 0.10.2.**

By default, Neovide requests to use VSync on the created window. `--no-vsync`
disables this behavior. The command line parameter takes priority over the
environment variable. If you don't enable vsync, then `g:neovide_refresh_rate`
will be used.

### Neovim Server

```sh
--server <ADDRESS>
```

Connects to the named pipe or socket at ADDRESS.

### WSL

```sh
--wsl
```

Runs neovim from inside wsl rather than as a normal executable.

### Neovim Binary

```sh
--neovim-bin or $NEOVIM_BIN
```

Sets where to find neovim's executable. If unset, neovide will try to find `nvim` on the `PATH`
environment variable instead. If you're running a Unix-alike, be sure that binary has the executable
permission bit set.

### Wayland / X11

```sh
--wayland-app-id <wayland_app_id> or $NEOVIDE_APP_ID
--x11-wm-class-instance <x11_wm_class_instance> or $NEOVIDE_WM_CLASS_INSTANCE
--x11-wm-class <x11_wm_class> or $NEOVIDE_WM_CLASS
```

On Linux/Unix, this alters the identification of the window to either X11 or the more modern
Wayland, depending on what you are running on.

### Working Directory

```sh
--chdir <path>
```

Start neovim in the specified working directory. This will impact neovim
arguments that use relative path names (e.g. file names), and the initial
working directory for all instances of neovim or terminal.
