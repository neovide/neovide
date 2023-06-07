# Command Line Reference

Neovide supports a few command line arguments for effecting things which couldn't be set using
normal vim variables.

`$` in front of a word refers to it being an "environment variable" which is checked for, some
settings only require it to be set in some way, some settings also use the contents.

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

### Log File

```sh
--log
```

Enables the log file for debugging purposes. This will write a file next to the executable
containing trace events which may help debug an issue.

### Maximized

```sh
--maximized or $NEOVIDE_MAXIMIZED
```

Maximize the window on startup, while still having decorations and the status bar of your OS
visible.

This is not the same as `g:neovide_fullscreen`, which runs Neovide in "exclusive fullscreen",
covering up the entire screen.

### Multigrid

```sh
--multigrid or $NEOVIDE_MULTIGRID
```

This enables neovim's multigrid functionality which will also enable floating window blurred
backgrounds and window animations. For now this is disabled due to some mouse input bugs upstream
([neovim/neovim/pull/12667](https://github.com/neovim/neovim/pull/12667),
[neovim/neovim/issues/15075](https://github.com/neovim/neovim/issues/15075)) and some
[floating window transparency issues](https://github.com/neovide/neovide/issues/720).

### No Fork

```sh
--nofork
```

By default, neovide detaches itself from the terminal. Instead of spawning a child process and
leaking it, be "blocking" and have the shell directly as parent process.

### No Idle

```sh
--noidle or $NEOVIDE_IDLE=0|1
```

With idle `on` (default), neovide won't render new frames when nothing is happening.

With idle `off` (e.g. with `--noidle` flag), neovide will constantly render new frames,
even when nothing changed. This takes more power and CPU time, but can possibly help
with frame timing issues.

### sRGB

```sh
--nosrgb, --srgb or $NEOVIDE_SRGB=0|1
```

Request sRGB support on the window. Neovide does not actually render with sRGB,
but it's still enabled by default on Windows to work around
[neovim/neovim/issues/907](https://github.com/neovim/neovim/issues/907). Other
platforms should not need it, but if you encounter either startup crashes or
wrong colors, you can try to swap the option. The command line parameter takes
priority over the environment variable.

### No Tabs

```sh
--notabs
```

By default, Neovide opens files given directly to Neovide (not NeoVim through `--`!) in multiple
tabs to avoid confusing new users. The option disables that and makes multiple given files to normal
buffers.

Note: Even if files are opened in tabs, they're buffers anyways. It's just about them being visible
or not.

### No VSync

```sh
--novsync, --vsync or $NEOVIDE_VSYNC=0|1
```

**Available since 0.10.2.**

By default, Neovide requests to use VSync on the created window. This
`--novsync` disables this behavior. The command line parameter takes priority
over the environment variable.

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
