# Config File

**Available since 0.11.0.**

Neovide also support configuration through a config file in [the toml format](https://toml.io).

## Settings priority

There are two types of settings:

1. Settings override these settings from the environment variables, but they can be overridden
   by command line arguments.
2. Runtime settings. These settings can be hot-reloaded in runtime.

## Location

| Platform | Location                                                                      |
| -------- | ----------------------------------------------------------------------------- |
| Linux    | `$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml` |
| macOS    | `$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml` |
| Windows  | `{FOLDERID_RoamingAppData}/neovide/config.toml`                               |

## Available settings

Settings currently available in the config file with default values:

```toml
wsl = false
no-multigrid = false
vsync = true
maximized = false
srgb = false
idle = true
neovim-bin = "/usr/bin/nvim" # in reality found dynamically on $PATH if unset
frame = "full"
```

Settings from environment variables can be found in [Command Line Reference](command-line-reference.md),
see that doc for details on what those settings do.

### Runtime settings

#### `Font`

**Available since 0.12.1.**

`[font]` table in configuration file contains:

- `normal`: `{ family = "string", style = "string" }` | `string`
- `bold`: `{ family = "string", style = "string" }` | `string`
- `italic`: `{ family = "string", style = "string" }` | `string`
- `bold_italic`: `{ family = "string", style = "string" }` | `string`
- `features`: `{ "<font>" = ["<string>"] }`
- `size`
- `width`
- `allow_float_size`
- `hinting`
- `edging`

Settings `size`, `width`, `allow_float_size`, `hinting` and `edging` can be found in
[Configuration](configuration.md)

Example:

```toml
[font]
normal = ["MonoLisa Nerd Font"]
size = 18

[font.features]
MonoLisa = [ "+ss01", "+ss07", "+ss11", "-calt", "+ss09", "+ss02", "+ss14", "+ss16", "+ss17" ]
```
