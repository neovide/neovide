# Config File

Neovide also support configuration through a config file in [the toml format](https://toml.io).

## Settings priority

Settings specified in the config file override settings from the environment variables, but are
overridden by command line arguments.

## Location

|Platform|Location|
|--------|-----|
|Linux|`$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml`|
|macOS|`$HOME/Library/Application Support/neovide/config.toml`|
|Windows|`{FOLDERID_RoamingAppData}/neovide/config.toml`|

[*source*](https://docs.rs/dirs/5.0.0/dirs/fn.config_dir.html)

## Available settings

Settings currently available in the config file are:

- `multigrid`
- `maximized`
- `vsync`
- `srgb`
- `no_idle`
- `neovim_bin`
- `frame`
- `geometry`

See [Command Line Reference](command-line-reference.md) for details on what those settings do.

*Note:* `srgb` and `vsync` are negations in command line arguments, but booleans in the config file.
So `--novsync` is equivalent to `vsync = false`.

## Example

This config changes all settings from their default values.

```toml
multigrid = true
vsync = false
maximized = true
srgb = false
no_idle = true
neovim_bin = "/bin/nvim"
frame = "None"
geometry = {width = 128, height = 72}
```
