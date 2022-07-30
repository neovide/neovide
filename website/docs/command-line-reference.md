# Command Line Reference

Neovide supports a few command line arguments for effecting things which couldn't be set using
normal vim variables.

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

### Multigrid

```sh
--multigrid or an environment variable declared named "NEOVIDE_MULTIGRID"
```

This enables neovim's multigrid functionality which will also enable floating window blurred
backgrounds and window animations. For now this is disabled due to some mouse input bugs upstream
([neovim/neovim/pull/12667](https://github.com/neovim/neovim/pull/12667),
[neovim/neovim/issues/15075](https://github.com/neovim/neovim/issues/15075)) and some
[floating window transparency issues](https://github.com/neovide/neovide/issues/720).

### Frameless

```sh
--frameless or an environment variable named NEOVIDE_FRAMELESS
```

Neovide without decorations. NOTE: Window cannot be moved nor resized after this.

### Geometry

```sh
--geometry=<width>x<height>
```

Sets the initial neovide window size in characters.

### No Fork

```sh
--nofork
```

By default, neovide detaches itself from the terminal. Instead of spawning a child process and
leaking it, be "blocking" and have the shell directly as parent process.

### No Tabs

```sh
--notabs
```

By default, Neovide opens files given directly to Neovide (not NeoVim through `--`!) in multiple
tabs to avoid confusing new users. The option disables that and makes multiple given files to normal
buffers.

Note: Even if files are opened in tabs, they're buffers anyways. It's just about them being visible
or not.

### WSL

```sh
--wsl
```

Runs neovim from inside wsl rather than as a normal executable.

### Neovim Binary

```sh
--neovim-bin
```

Sets where to find neovim's executable. If unset, neovide will try to find `nvim` on the `PATH`
environment variable instead. If you're running a Unix-alike, be sure that binary has the executable
permission bit set.

### Log File

```sh
--log
```

Enables the log file for debugging purposes. This will write a file next to the executable
containing trace events which may help debug an issue.

### Wayland / X11

```sh
--wayland-app-id <wayland_app_id> or an environment variable called NEOVIDE_APP_ID
--x11-wm-class <x11_wm_class> or an environment variable called NEOVIDE_WM_CLASS
```

On Linux/Unix, this alters the identification of the window to either X11 or the more modern
Wayland, depending on what you are running on.
