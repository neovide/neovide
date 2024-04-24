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
title-hidden = true
tabs = true
```

Settings from environment variables can be found in [Command Line Reference](command-line-reference.md),
see that doc for details on what those settings do.

### Runtime settings

#### `Font`

**Available since 0.12.1.**

`[font]` table in configuration file contains:

- `normal`: required, `FontDescription`
- `bold`: optional, `SecondaryFontDescription`
- `italic`: optional, `SecondaryFontDescription`
- `bold_italic`: optional, `SecondaryFontDescription`
- `features`: optional, `{ "<font>" = ["<string>"] }`
- `size`: required,
- `width`: optional,
- `hinting`: optional,
- `edging`: optional,

Settings `size`, `width`, `hinting` and `edging` can be found in
[Configuration](configuration.md).

- `FontDescription` can be:
  - a table with two keys `family` and `style`, `family` is required, `style` is optional,
  - a string, indicate the font family,
  - an array of string or tables in previous two forms.
- `SecondaryFontDescription` can be:
  - a table with two keys `family` and `style`, both are optional,
  - a string, indicate the font family,
  - an array of string or tables in previous two forms.
- Font styles consist of zero or more space separated parts, each parts can be:
  - pre-defined style name
    - weight: `Thin`, `ExtraLight`, `Light`, `Normal`, `Medium`, `SemiBold`, `Bold`,
      `ExtraBold`, `Black`, `ExtraBlack`
    - slant: `Italic`, `Oblique`
  - variable font weight: `W<weight>`, e.g. `W100`, `W200`, `W300`, `W400`, `W500`, `W600`,
    `W700`, `W800`, `W900`
- Font features are a table with font family as key and an array of string as value, each
  string is a font feature.
  - Font feature is a string with format `+<feature>`, `-<feature>` or `<feature>=<value>`,
    e.g. `+ss01`, `-calt`, `ss02=2`. `+<feature>` is a shorthand for `<feature>=1`,
    `-<feature>` is a shorthand for `<feature>=0`.

Example:

```toml
[font]
normal = ["MonoLisa Nerd Font"]
size = 18

[font.features]
MonoLisa = [ "+ss01", "+ss07", "+ss11", "-calt", "+ss09", "+ss02", "+ss14", "+ss16", "+ss17" ]
```
