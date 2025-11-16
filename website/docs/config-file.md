# Config File

**Available since 0.11.0.**

Neovide also support configuration through a config file in [the toml format](https://toml.io).

## Settings priority

There are two types of settings:

1. Settings override these settings from the environment variables, but they can be overridden
   by command line arguments.
2. Runtime settings. These settings can be hot-reloaded in runtime.

## Location

| Platform | Location                                                                      | Example                                                        |
| -------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------- |
| Linux    | `$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml` | `/home/alice/.config/neovide/config.toml`                      |
| macOS    | `$XDG_CONFIG_HOME/neovide/config.toml` or `$HOME/.config/neovide/config.toml` | `/Users/Alice/Library/Application Support/neovide/config.toml` |
| Windows  | `{FOLDERID_RoamingAppData}/neovide/config.toml`                               | `C:\Users\Alice\AppData\Roaming/neovide/config.toml`           |

You may use a different location by modifying the `$NEOVIDE_CONFIG` environment variable to be
a full path to a `config.toml` file (doesn't explicitly have to be called `config.toml`
however.)

## Available settings

Settings currently available in the config file with default values:

```toml
backtraces_path = "/path/to/neovide_backtraces.log" # see below for the default platform specific location
chdir = "/path/to/dir"
fork = false
frame = "full"
idle = true
icon = "/full/path/to/neovide.ico" # Example path. Default icon is bundled. Use .icns on macOS.
maximized = false
mouse-cursor-icon = "arrow"
neovim-bin = "/usr/bin/nvim" # in reality found dynamically on $PATH if unset
no-multigrid = false
srgb = false # platform-specific: false (Linux/macOS) or true (Windows)
tabs = true
macos-native-tabs = false # macOS only
macos-pinned-hotkey = "ctrl+shift+z" # macOS only
macos-switcher-hotkey = "ctrl+shift+n" # macOS only
macos-tab-prev-hotkey = "cmd+shift+[" # macOS only
macos-tab-next-hotkey = "cmd+shift+]" # macOS only
theme = "auto"
title-hidden = false
vsync = true
wsl = false

[font]
normal = [] # Will use the bundled Fira Code Nerd Font by default
size = 14.0

[box-drawing]
# "font-glyph", "native" or "selected-native"
mode = "font-glyph"

[box-drawing.sizes]
default = [2, 4]  # Thin and thick values respectively, for all sizes
```

Refer to [Command Line Reference](command-line-reference.md) for details about the config settings
listed above.

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
- `underline_offset`: optional

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
- `underline_offset` is a float that defines the offset between the character baseline and the
  underline.
  - If not specified, it will be decided automatically, either if the font contains the required
    metrics, or `-1.0` by default.
  - Positive underline offset values will move the underline below the baseline, while negative
    values move it above.

Example:

```toml
[font]
normal = ["MonoLisa Nerd Font"]
size = 18

[font.features]
"MonoLisa Nerd Font" = [ "+ss01", "+ss07", "+ss11", "-calt", "+ss09", "+ss02", "+ss14" ]
```

Specify font weight:

```toml
[font]
size = 19
hinting = "full"
edging = "antialias"

[[font.normal]]
family = "JetBrainsMono Nerd Font Propo"
style = "W400"

# You can set a different font for fallback
[[font.normal]]
family = "Noto Sans CJK SC"
style = "Normal"

[[font.bold]]
family = "JetBrainsMono Nerd Font Propo"
style = "W600"

# No need to specify fallback in every variant, if omitted or specified here
# but not found, it will fallback to normal font with this weight which is bold
# in this case.
[[font.bold]]
family = "Noto Sans CJK SC"
style = "Bold"
```

#### Box Drawing

The Unicode standard defines several code points that are useful to draw [boxes, diagrams or are
otherwise decorations](https://en.wikipedia.org/wiki/Box_Drawing). A font file can include graphical
representation for several of these code points (glyphs). For example, [Nerd
Fonts](https://www.nerdfonts.com/) is a collection of font faces that have been patched to include
glyphs for several box drawing code points (and many other use-cases).

When Neovide renders these glyphs, some glyphs might not line up correctly or might have gaps
between adjacent cells, breaking visual continuity. This is especially pronounced when using the
[linespace](./configuration.md#line-spacing) configuration option to add spacing between lines.

Neovide has support for native rendering (i.e ignore the glyph data in the font) for a subset of
these glyphs to avoid this problem. You can configure this via:

```toml
[box-drawing]
# "font-glyph", "native" or "selected-native"
mode = "native"
# selected = "🮐🮑🮒"
```

- `font-glyph` uses the glyph data in the font file.
- `native` (default) turns on native rendering for all supported box drawing glyphs.
- `selected-native` turns on native rendering for only code points specified in the `selected`
  setting.

The width of the lines drawn can be further controlled using the following settings:

```toml
[box-drawing.sizes]
default = [1, 3]  # Thin and thick values respectively, below 12px
12 = [1, 2]       # 12px to 13.9999px
14 = [2, 4]
18 = [3, 6]
```

The `sizes` settings maps font sizes the thickness (in pixels) for thin and thick lines
respectively. For example, if you are using a font with size 15px and with the above settings,
Neovide to draw thin lines with width 2px and thick lines with width 4px. These settings only needs
changing if you find that at certain font sizes the box characters seem too thick or too thin to
your liking. Only `default` is required and overrides for specific sizes is optional.

**NOTE:** The sizes are specified in pixels unlike font size, which is specified in points. The
reason for that, is to give a more controllable configuration when you are using different DPI
settings. To convert from pt to pixels you can use the following formula `pt_size * (96/72) *
scale`, so if you are using a 10.5 pt size font with a scale factor of 1.5, then it will become
`10.5 pt * (96/72) * 1.5 = 21 px`. You also have to add the `linespace` setting if you use that.

The default is 2 pixels for thin and 4 pixels for thick lines regardless of the font size, which
corresponds to the settings below.

```toml
[box-drawing.sizes]
default = [2, 4]  # Thin and thick values respectively, for all sizes
```

#### backtraces_path

**Available since 0.14.0.**

If Neovide crashes, it will write a file named `neovide_backtraces.log` into
this location, with more information about the crash. This can alternatively be
configured through the environment variable `NEOVIDE_BACKTRACES`, which is
useful if the crash happens before the config file is read for example.

The default location is the following:

| Platform | Location                                       | Example                                            |
| -------- | ---------------------------------------------- | -------------------------------------------------- |
| Linux    | `$XDG_DATA_HOME or $HOME/.local/share/neovide` | `/home/alice/.local/share/neovide`                 |
| macOS    | `$HOME/Library/Application Support/neovide`    | `/Users/Alice/Library/Application Support/neovide` |
| Windows  | `{FOLDERID_LocalAppData}\neovide`              | `C:\Users\Alice\AppData\Local\neovide`             |
