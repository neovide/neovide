# Config File

Neovide also support configuration through a config file in [the toml format](https://toml.io).

## Settings priority

Settings specified in the config file override settings from the environment variables, but are
overridden by command line arguments.

## Location

|Platform|Location|
|--------|-----|
|Linux|`$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml`|
|macOS|`$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml`|
|Windows|`{FOLDERID_RoamingAppData}/neovide/config.toml`|

## Available settings

Settings currently available in the config file with default values:

```toml
multigrid = false
vsync = true
maximized = false
srgb = false
idle = true
neovim_bin = "/usr/bin/nvim"
frame = "Full"
```

See [Command Line Reference](command-line-reference.md) for details on what those settings do.
