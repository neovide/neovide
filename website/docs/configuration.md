# Configuration

## Global Vim Settings

Neovide supports settings via global variables with a neovide prefix. They enable configuring many
parts of the editor and support dynamically changing them at runtime.

### `init.vim` and `init.lua` helpers

#### Hello, is this Neovide?

Not really a configuration option, but `g:neovide` only exists and is set to `v:true` if this Neovim
is in Neovide. Useful for configuring things only for Neovide in your `init.vim`/`init.lua`:

VimScript:

```vim
if exists("g:neovide")
    " Put anything you want to happen only in Neovide here
endif
```

Lua:

```lua
if vim.g.neovide then
    -- Put anything you want to happen only in Neovide here
end
```

### Display

#### Font

VimScript:

```vim
set guifont=Source\ Code\ Pro:h14
```

Lua:

```lua
vim.o.guifont = "Source Code Pro:h14" -- text below applies for VimScript
```

Controls the font used by Neovide. Also check [the config file](./config-file.md) to see how to
configure features. This is the only setting which is actually controlled through an option, and
as such it's also documented in `:h guifont`. But to sum it up and also add Neovide's extension:

- The basic format is `Primary\ Font,Fallback\ Font\ 1,Fallback\ Font\ 2:option1:option2:option3`,
  while you can have as many fallback fonts as you want (even 0) and as many options as you want
  (also even 0).
- Fonts
  - are separated with `,` (commas).
  - can contain spaces by either escaping them or using `_` (underscores).
- Options
  - apply to all fonts at once.
  - are separated from the fonts and themselves through `:` (colons).
  - can be one of the following:
    - `hX` — Sets the font size to `X` points, while `X` can be any (even floating-point) number.
    - `wX` (available since 0.11.2) — Sets the width **relative offset** to be `X` points, while `X`
        can be again any number. Negative values shift characters closer together, positive values
        shift them further apart.
    - `b` — Sets the font **bold**.
    - `i` — Sets the font _italic_.
    - `#e-X` (available since 0.10.2) — Sets edge pixels to be drawn opaquely or
      with partial transparency, while `X` is a type of edging:
      - antialias (default)
      - subpixelantialias
      - alias
    - `#h-X` (available since 0.10.2) - Sets level of glyph outline adjustment, while `X` is
      a type of hinting:
      - full (default)
      - normal
      - slight
      - none
- Some examples:
  - `Hack,Noto_Color_Emoji:h12:b` — Hack at size 12 in bold, with Noto Color Emoji as fallback
    should Hack fail to contain any glyph.
  - `Roboto_Mono_Light:h10` — Roboto Mono Light at size 10.
  - `Hack:h14:i:#e-subpixelantialias:#h-none`

#### Line spacing

VimScript:

```vim
set linespace=0
```

Lua:

```lua
vim.opt.linespace = 0
```

Controls spacing between lines, may also be negative.

#### Scale

VimScript:

```vim
let g:neovide_scale_factor = 1.0
```

Lua:

```lua
vim.g.neovide_scale_factor = 1.0
```

**Available since 0.10.2.**

In addition to setting the font itself, this setting allows to change the scale without changing the
whole font definition. Very useful for presentations. See [the FAQ section about
this][scale-runtime] for a nice recipe to bind this to a hotkey.

[scale-runtime]: faq.md#how-can-i-dynamically-change-the-scale-at-runtime

#### Text Gamma and Contrast

VimScript:

```vim
let g:neovide_text_gamma = 0.0
let g:neovide_text_contrast = 0.5
```

Lua:

```lua
vim.g.neovide_text_gamma = 0.0
vim.g.neovide_text_contrast = 0.5
```

**Available since 0.13.0.**

You can fine tune the gamma and contrast of the text to your liking. The defaults is a good
compromise that gives readable text on all backgrounds and an accurate color representation. But if
that doesn't suit you, and you want to emulate the Alacritty font rendering for example you can use
a gamma of 0.8 and a contrast of 0.1.

Note a gamma of 0.0, means standard sRGB gamma or 2.2. Also note that these settings don't
necessarily apply immediately due to caching of the fonts.

#### Padding

VimScript:

```vim
let g:neovide_padding_top = 0
let g:neovide_padding_bottom = 0
let g:neovide_padding_right = 0
let g:neovide_padding_left = 0
```

Lua:

```lua
vim.g.neovide_padding_top = 0
vim.g.neovide_padding_bottom = 0
vim.g.neovide_padding_right = 0
vim.g.neovide_padding_left = 0
```

**Available since 0.10.4.**

Controls the space between the window border and the actual Neovim, which is filled with the
background color instead.

#### Background Color (**Deprecated**, Currently macOS only)

This configuration is deprecated now and might be removed in the future. In
[#2168](https://github.com/neovide/neovide/issues/2168), we have made Neovide control the title bar
color itself. The color of title bar now honors [`neovide_transparency`](#transparency). If you want
a transparent title bar, setting `neovide_transparency` is sufficient.

VimScript:

```vim
" g:neovide_transparency should be 0 if you want to unify transparency of content and title bar.
let g:neovide_transparency = 0.0
let g:transparency = 0.8
let g:neovide_background_color = '#0f1117'.printf('%x', float2nr(255 * g:transparency))
```

Lua:

```lua
-- Helper function for transparency formatting
local alpha = function()
  return string.format("%x", math.floor(255 * vim.g.transparency or 0.8))
end
-- g:neovide_transparency should be 0 if you want to unify transparency of content and title bar.
vim.g.neovide_transparency = 0.0
vim.g.transparency = 0.8
vim.g.neovide_background_color = "#0f1117" .. alpha()
```

**Available since 0.10.**
**Deprecated in 0.12.2.**

![BackgroundColor](assets/BackgroundColor.png)

Setting `g:neovide_background_color` to a value that can be parsed by
[csscolorparser-rs](https://github.com/mazznoer/csscolorparser-rs) will set the color of the whole
window to that value.

Note that `g:neovide_transparency` should be 0 if you want to unify transparency of content and
title bar.

#### Window Blur (Currently macOS only)

VimScript:

```vim
let g:neovide_window_blurred = v:true
```

Lua:

```lua
vim.g.neovide_window_blurred = true
```

**Available since 0.12.**

Setting `g:neovide_window_blurred` toggles the window blur state.

The blurred level respects the `g:neovide_transparency` value between 0.0 and 1.0.

#### Floating Blur Amount

VimScript:

```vim
let g:neovide_floating_blur_amount_x = 2.0
let g:neovide_floating_blur_amount_y = 2.0
```

Lua:

```lua
vim.g.neovide_floating_blur_amount_x = 2.0
vim.g.neovide_floating_blur_amount_y = 2.0
```

**Available since 0.9.**

Setting `g:neovide_floating_blur_amount_x` and `g:neovide_floating_blur_amount_y` controls the blur
radius on the respective axis for floating windows.

#### Floating Shadow

VimScript:

```vim
let g:neovide_floating_shadow = v:true
let g:neovide_floating_z_height = 10
let g:neovide_light_angle_degrees = 45
let g:neovide_light_radius = 5
```

Lua:

```lua
vim.g.neovide_floating_shadow = true
vim.g.neovide_floating_z_height = 10
vim.g.neovide_light_angle_degrees = 45
vim.g.neovide_light_radius = 5
```

**Available since 0.12.0.**

Setting `g:neovide_floating_shadow` to false will disable the shadow borders for floating windows.
The other variables configure the shadow in various ways:

- `g:neovide_floating_z_height` sets the virtual height of the floating window from the ground plane
- `g:neovide_light_angle_degrees` sets the angle from the screen normal of the casting light
- `g:neovide_light_radius` sets the radius of the casting light

#### Transparency

VimScript:

```vim
let g:neovide_transparency = 0.8
```

Lua:

```lua
vim.g.neovide_transparency = 0.8
```

![Transparency](assets/Transparency.png)

Setting `g:neovide_transparency` to a value between 0.0 and 1.0 will set the opacity of the window
to that value.

#### Show Border (Currently macOS only)

VimScript:

```vim
let g:neovide_show_border = v:true
```

Lua:

```lua
vim.g.neovide_show_border = true
```

Draw a grey border around opaque windows only.

Default: `false`

#### Scroll Animation Length

VimScript:

```vim
let g:neovide_scroll_animation_length = 0.3
```

Lua:

```lua
vim.g.neovide_scroll_animation_length = 0.3
```

Sets how long the scroll animation takes to complete, measured in seconds. Note that the timing is
not completely accurate and might depend slightly on have far you scroll, so experimenting is
encouraged in order to tune it to your liking.

#### Far scroll lines

**Available since 0.12.0.**

VimScript:

```vim
let g:neovide_scroll_animation_far_lines = 1
```

Lua:

```lua
vim.g.neovide_scroll_animation_far_lines = 1
```

When scrolling more than one screen at a time, only this many lines at the end of the scroll action
will be animated. Set it to 0 to snap to the final position without any animation, or to something
big like 9999 to always scroll the whole screen, much like Neovide <= 0.10.4 did.

#### Hiding the mouse when typing

VimScript:

```vim
let g:neovide_hide_mouse_when_typing = v:false
```

Lua:

```lua
vim.g.neovide_hide_mouse_when_typing = false
```

By setting this to `v:true`, the mouse will be hidden as soon as you start typing. This setting
only affects the mouse if it is currently within the bounds of the neovide window. Moving the
mouse makes it visible again.

#### Underline automatic scaling

VimScript:

```vim
let g:neovide_underline_stroke_scale = 1.0
```

Lua:

```lua
vim.g.neovide_underline_stroke_scale = 1.0
```

**Available since 0.12.0.**

Setting `g:neovide_underline_stroke_scale` to a floating point will increase or decrease the stroke
width of the underlines (including undercurl, underdash, etc.). If the scaled stroke width is less
than 1, it is clamped to 1 to prevent strange aliasing.

**Note**: This is currently glitchy if the scale is too large, and leads to some underlines being
clipped by the line of text below.

#### Theme

VimScript:

```vim
let g:neovide_theme = 'auto'
```

Lua:

```lua
vim.g.neovide_theme = 'auto'
```

**Available since 0.11.0.**

Set the [`background`](https://neovim.io/doc/user/options.html#'background') option when Neovide
starts. Possible values: _light_, _dark_, _auto_. On systems that support it, _auto_ will mirror the
system theme, and will update `background` when the system theme changes.

### Functionality

#### Refresh Rate

VimScript:

```vim
let g:neovide_refresh_rate = 60
```

Lua:

```lua
vim.g.neovide_refresh_rate = 60
```

Setting `g:neovide_refresh_rate` to a positive integer will set the refresh rate of the app. This is
limited by the refresh rate of your physical hardware, but can be lowered to increase battery life.

This setting is only effective when not using vsync, for example by passing `--no-vsync` on the
commandline.

#### Idle Refresh Rate

VimScript:

```vim
let g:neovide_refresh_rate_idle = 5
```

Lua:

```lua
vim.g.neovide_refresh_rate_idle = 5
```

**Available since 0.10.**

Setting `g:neovide_refresh_rate_idle` to a positive integer will set the refresh rate of the app
when it is not in focus.

This might not have an effect on every platform (e.g. Wayland).

#### No Idle

VimScript:

```vim
let g:neovide_no_idle = v:true
```

Lua:

```lua
vim.g.neovide_no_idle = true
```

Setting `g:neovide_no_idle` to a boolean value will force neovide to redraw all the time. This can
be a quick hack if animations appear to stop too early.

#### Confirm Quit

VimScript:

```vim
let g:neovide_confirm_quit = v:true
```

Lua:

```lua
vim.g.neovide_confirm_quit = true
```

If set to `true`, quitting while having unsaved changes will require confirmation. Enabled by
default.

#### Fullscreen

VimScript:

```vim
let g:neovide_fullscreen = v:true
```

Lua:

```lua
vim.g.neovide_fullscreen = true
```

Setting `g:neovide_fullscreen` to a boolean value will set whether the app should take up the entire
screen. This uses the so called "windowed fullscreen" mode that is sometimes used in games which
want quick window switching.

#### Remember Previous Window Size

VimScript:

```vim
let g:neovide_remember_window_size = v:true
```

Lua:

```lua
vim.g.neovide_remember_window_size = true
```

Setting `g:neovide_remember_window_size` to a boolean value will determine whether the window size
from the previous session or the default size will be used on startup. The commandline option
`--size` will take priority over this value.

#### Profiler

VimScript:

```vim
let g:neovide_profiler = v:false
```

Lua:

```lua
vim.g.neovide_profiler = false
```

Setting this to `v:true` enables the profiler, which shows a frametime graph in the upper left
corner.

### Input Settings

#### macOS Option Key is Meta

Possible values are `both`, `only_left`, `only_right`, `none`. Set to `none` by default.

VimScript:

```vim
let g:neovide_input_macos_option_key_is_meta = 'only_left'
```

Lua:

```lua
vim.g.neovide_input_macos_option_key_is_meta = 'only_left'
```

**Available since 0.13.0.**

Interprets <kbd>Alt</kbd> + <kbd>whatever</kbd> actually as `<M-whatever>`, instead of sending the
actual special character to Neovim.

#### IME

VimScript:

```vim
let g:neovide_input_ime = v:true
```

Lua:

```lua
vim.g.neovide_input_ime = true
```

**Available since 0.11.0.**

This lets you disable the IME input. For example, to only enables IME in input mode and when
searching, so that you can navigate normally, when typing some East Asian languages, you can add
a few auto commands:

```vim
augroup ime_input
    autocmd!
    autocmd InsertLeave * execute "let g:neovide_input_ime=v:false"
    autocmd InsertEnter * execute "let g:neovide_input_ime=v:true"
    autocmd CmdlineLeave [/\?] execute "let g:neovide_input_ime=v:false"
    autocmd CmdlineEnter [/\?] execute "let g:neovide_input_ime=v:true"
augroup END
```

```lua
local function set_ime(args)
    if args.event:match("Enter$") then
        vim.g.neovide_input_ime = true
    else
        vim.g.neovide_input_ime = false
    end
end

local ime_input = vim.api.nvim_create_augroup("ime_input", { clear = true })

vim.api.nvim_create_autocmd({ "InsertEnter", "InsertLeave" }, {
    group = ime_input,
    pattern = "*",
    callback = set_ime
})

vim.api.nvim_create_autocmd({ "CmdlineEnter", "CmdlineLeave" }, {
    group = ime_input,
    pattern = "[/\\?]",
    callback = set_ime
})
```

#### Touch Deadzone

VimScript:

```vim
let g:neovide_touch_deadzone = 6.0
```

Lua:

```lua
vim.g.neovide_touch_deadzone = 6.0
```

Setting `g:neovide_touch_deadzone` to a value equal or higher than 0.0 will set how many pixels the
finger must move away from the start position when tapping on the screen for the touch to be
interpreted as a scroll gesture.

If the finger stayed in that area once lifted or the drag timeout happened, however, the touch will
be interpreted as tap gesture and the cursor will move there.

A value lower than 0.0 will cause this feature to be disabled and _all_ touch events will be
interpreted as scroll gesture.

#### Touch Drag Timeout

VimScript:

```vim
let g:neovide_touch_drag_timeout = 0.17
```

Lua:

```lua
vim.g.neovide_touch_drag_timeout = 0.17
```

Setting `g:neovide_touch_drag_timeout` will affect how many seconds the cursor has to stay inside
`g:neovide_touch_deadzone` in order to begin "dragging"

Once started, the finger can be moved to another position in order to form a visual selection. If
this happens too often accidentally to you, set this to a higher value like `0.3` or `0.7`.

### Cursor Settings

#### Animation Length

<p align="center">
  <img alt="Short Cursor Animation Length", src="./assets/ShortCursorAnimationLength.gif" width="47%">
&nbsp; &nbsp;
  <img alt="Long Cursor Animation Length", src="./assets/LongCursorAnimationLength.gif" width="47%">
</p>

VimScript:

```vim
let g:neovide_cursor_animation_length = 0.13
```

Lua:

```lua
vim.g.neovide_cursor_animation_length = 0.13
```

Setting `g:neovide_cursor_animation_length` determines the time it takes for the cursor to complete
it's animation in seconds. Set to `0` to disable.

#### Animation Trail Size

<p align="center">
  <img alt="Short Cursor Trail Length", src="./assets/ShortCursorTrailLength.gif" width="47%">
&nbsp; &nbsp;
  <img alt="Long Cursor Trail Length", src="./assets/LongCursorTrailLength.gif" width="47%">
</p>

VimScript:

```vim
let g:neovide_cursor_trail_size = 0.8
```

Lua:

```lua
vim.g.neovide_cursor_trail_size = 0.8
```

Setting `g:neovide_cursor_trail_size` determines how much the trail of the cursor lags behind the
front edge.

#### Antialiasing

VimScript:

```vim
let g:neovide_cursor_antialiasing = v:true
```

Lua:

```lua
vim.g.neovide_cursor_antialiasing = true
```

Enables or disables antialiasing of the cursor quad. Disabling may fix some cursor visual issues.

#### Animate in insert mode

VimScript:

```vim
let g:neovide_cursor_animate_in_insert_mode = v:true
```

Lua:

```lua
vim.g.neovide_cursor_animate_in_insert_mode = true
```

If disabled, when in insert mode (mostly through `i` or `a`), the cursor will move like in other
programs and immediately jump to its new position.

#### Animate switch to command line

VimScript:

```vim
let g:neovide_cursor_animate_command_line = v:true
```

Lua:

```lua
vim.g.neovide_cursor_animate_command_line = true
```

If disabled, the switch from editor window to command line is non-animated, and the cursor jumps
between command line and editor window immediately. Does **not** influence animation inside of the
command line.

#### Unfocused Outline Width

VimScript:

```vim
let g:neovide_cursor_unfocused_outline_width = 0.125
```

Lua:

```lua
vim.g.neovide_cursor_unfocused_outline_width = 0.125
```

Specify cursor outline width in `em`s. You probably want this to be a positive value less than 0.5.
If the value is \<=0 then the cursor will be invisible. This setting takes effect when the editor
window is unfocused, at which time a block cursor will be rendered as an outline instead of as a
full rectangle.

#### Animate cursor blink

VimScript:

```vim
let g:neovide_cursor_smooth_blink = v:false
```

Lua:

```lua
vim.g.neovide_cursor_smooth_blink = false
```

If enabled, the cursor will smoothly animate the transition between the cursor's on and off state.
The built in `guicursor` neovim option needs to be configured to enable blinking by having a value
set for both `blinkoff`, `blinkon` and `blinkwait` for this setting to apply.

### Cursor Particles

There are a number of vfx modes you can enable which produce particles behind the cursor. These are
enabled by setting `g:neovide_cursor_vfx_mode` to one of the following constants.

#### None at all

VimScript:

```vim
let g:neovide_cursor_vfx_mode = ""
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = ""
```

The default, no particles at all.

#### Railgun

<img src="./assets/Railgun.gif" alt="Railgun" width=550>

VimScript:

```vim
let g:neovide_cursor_vfx_mode = "railgun"
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = "railgun"
```

#### Torpedo

<img src="./assets/Torpedo.gif" alt="Torpedo" width=550>

VimScript:

```vim
let g:neovide_cursor_vfx_mode = "torpedo"
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = "torpedo"
```

#### Pixiedust

<img src="./assets/Pixiedust.gif" alt="Pixiedust" width=550>

VimScript:

```vim
let g:neovide_cursor_vfx_mode = "pixiedust"
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = "pixiedust"
```

#### Sonic Boom

<img src="./assets/Sonicboom.gif" alt="Sonicboom" width=550>

VimScript:

```vim
let g:neovide_cursor_vfx_mode = "sonicboom"
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = "sonicboom"
```

#### Ripple

<img src="./assets/Ripple.gif" alt="Ripple" width=550>

VimScript:

```vim
let g:neovide_cursor_vfx_mode = "ripple"
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = "ripple"
```

#### Wireframe

<img src="./assets/Wireframe.gif" alt="Wireframe" width=550>

VimScript:

```vim
let g:neovide_cursor_vfx_mode = "wireframe"
```

Lua:

```lua
vim.g.neovide_cursor_vfx_mode = "wireframe"
```

### Particle Settings

Options for configuring the particle generation and behavior.

#### Particle Opacity

VimScript:

```vim
let g:neovide_cursor_vfx_opacity = 200.0
```

Lua:

```lua
vim.g.neovide_cursor_vfx_opacity = 200.0
```

Sets the transparency of the generated particles.

#### Particle Lifetime

VimScript:

```vim
let g:neovide_cursor_vfx_particle_lifetime = 1.2
```

Lua:

```lua
vim.g.neovide_cursor_vfx_particle_lifetime = 1.2
```

Sets the amount of time the generated particles should survive.

#### Particle Density

VimScript:

```vim
let g:neovide_cursor_vfx_particle_density = 7.0
```

Lua:

```lua
vim.g.neovide_cursor_vfx_particle_density = 7.0
```

Sets the number of generated particles.

#### Particle Speed

VimScript:

```vim
let g:neovide_cursor_vfx_particle_speed = 10.0
```

Lua:

```lua
vim.g.neovide_cursor_vfx_particle_speed = 10.0
```

Sets the speed of particle movement.

#### Particle Phase

VimScript:

```vim
let g:neovide_cursor_vfx_particle_phase = 1.5
```

Lua:

```lua
vim.g.neovide_cursor_vfx_particle_phase = 1.5
```

Only for the `railgun` vfx mode.

Sets the mass movement of particles, or how individual each one acts. The higher the value, the less
particles rotate in accordance to each other, the lower, the more line-wise all particles become.

#### Particle Curl

VimScript:

```vim
let g:neovide_cursor_vfx_particle_curl = 1.0
```

Lua:

```lua
vim.g.neovide_cursor_vfx_particle_curl = 1.0
```

Only for the `railgun` vfx mode.

Sets the velocity rotation speed of particles. The higher, the less particles actually move and look
more "nervous", the lower, the more it looks like a collapsing sine wave.

<!--
  vim: textwidth=100
-->
