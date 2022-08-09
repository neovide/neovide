# Configuration

## Global Vim Settings

Neovide supports settings via global variables with a neovide prefix. They enable configuring many
parts of the editor and support dynamically changing them at runtime.

### Functionality

#### Hello, is this Neovide?

Not really a configuration option, but `g:neovide` only exists and is set to `v:true` if this Neovim
is in Neovide. It's not set else. Useful for configuring things only for Neovide in your `init.vim`:

```lua
if exists("g:neovide")
    " Put anything you want to happen only in Neovide here
endif
```

#### Refresh Rate

```vim
let g:neovide_refresh_rate=60
```

Setting `g:neovide_refresh_rate` to a positive integer will set the refresh rate of the app. This is
limited by the refresh rate of your physical hardware, but can be lowered to increase battery life.

#### Idle Refresh Rate

```vim
let g:neovide_refresh_rate_idle=5
```

Setting `g:neovide_refresh_rate_idle` to a positive integer will set the refresh rate of the app when
it is not in focus.

This might not have an effect on every platform (e.g. Wayland).

#### Transparency

```vim
let g:neovide_transparency=0.8
```

![Transparency](assets/Transparency.png)

Setting `g:neovide_transparency` to a value between 0.0 and 1.0 will set the opacity of the window
to that value.

#### Background Color (Currently macOS only)

```vim
" g:neovide_transparency should be 0 if you want to unify transparency of content and title bar.
let g:neovide_transparency=0.0
let g:transparency = 0.8
let g:neovide_background_color = '#0f1117'.printf('%x', float2nr(255 * g:transparency))
```

![BackgroundColor](assets/BackgroundColor.png)

Setting `g:neovide_background_color` to a value that can be parsed by
[csscolorparser-rs](https://github.com/mazznoer/csscolorparser-rs) will set the color of the whole
window to that value.

Note that `g:neovide_transparency` should be 0 if you want to unify transparency of content and
title bar.

#### Floating Blur Amount

```vim
let g:neovide_floating_blur_amount_x = 2.0
let g:neovide_floating_blur_amount_y = 2.0
```

**Available since 0.9.**

Setting `g:neovide_floating_blur_amount_x` and `g:neovide_floating_blur_amount_y` controls the blur
radius on the respective axis for floating windows.

#### Scroll Animation Length

```vim
let g:neovide_scroll_animation_length = 0.3
```

Sets how long the scroll animation takes to complete, measured in seconds.

#### No Idle

```vim
let g:neovide_no_idle=v:true
```

Setting `g:neovide_no_idle` to a boolean value will force neovide to redraw all the time. This can
be a quick hack if animations appear to stop too early.

#### Fullscreen

```vim
let g:neovide_fullscreen=v:true
```

Setting `g:neovide_fullscreen` to a boolean value will set whether the app should take up the entire
screen. This uses the so called "windowed fullscreen" mode that is sometimes used in games which
want quick window switching.

#### Confirm Quit

```vim
let g:neovide_confirm_quit=v:false
" or
let g:neovide_confirm_quit=0
```

If set to `true`, quitting while having unsaved changes will require confirmation.
Enabled by default.

#### Remember Previous Window Size

```vim
let g:neovide_remember_window_size = v:true
```

Setting `g:neovide_remember_window_size` to a boolean value will determine whether the window size
from the previous session or the default size will be used on startup. The commandline option
`--geometry` will take priority over this value.

#### Profiler

```vim
let g:neovide_profiler = v:false
```

Setting this to `v:true` enables the profiler, which shows a frametime graph in the upper left
corner.

### Input Settings

#### Use Logo Key

```vim
let g:neovide_input_use_logo=v:false  " v:true on macOS
```

Setting `g:neovide_input_use_logo` to a boolean value will change how logo key (also known as
[super key](<https://en.wikipedia.org/wiki/Super_key_(keyboard_button)>),
[command key](https://en.wikipedia.org/wiki/Command_key) or
[windows key](https://en.wikipedia.org/wiki/Windows_key)) is handled, allowing all key combinations
containing logo to be forwarded to neovim. On MacOS, this defaults to `true` (so that e.g. `cmd+v`
works for pasting with respective setup of `init.vim`), and to `false` for other platforms (that
typically use e.g. `ctrl+v` for pasting).

#### macOS Alt is Meta

```vim
let g:neovide_input_macos_alt_is_meta=v:false
```

**Unreleased yet.**

Interprets <kbd>Alt</kbd> + <kbd>whatever</kbd> actually as `<M-whatever>`, instead of sending the
actual special character to Neovim.

#### Touch Deadzone

```vim
let g:neovide_touch_deadzone=6.0
```

Setting `g:neovide_touch_deadzone` to a value equal or higher than 0.0 will set how many pixels the
finger must move away from the start position when tapping on the screen for the touch to be
interpreted as a scroll gesture.

If the finger stayed in that area once lifted or the drag timeout happened, however, the touch will
be interpreted as tap gesture and the cursor will move there.

A value lower than 0.0 will cause this feature to be disabled and _all_ touch events will be
interpreted as scroll gesture.

#### Touch Drag Timeout

```vim
let g:neovide_touch_drag_timeout=0.17
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

```vim
let g:neovide_cursor_animation_length=0.13
```

Setting `g:neovide_cursor_animation_length` determines the time it takes for the cursor to complete
it's animation in seconds. Set to `0` to disable.

#### Animation Trail Length

<p align="center">
  <img alt="Short Cursor Trail Length", src="./assets/ShortCursorTrailLength.gif" width="47%">
&nbsp; &nbsp;
  <img alt="Long Cursor Trail Length", src="./assets/LongCursorTrailLength.gif" width="47%">
</p>

```vim
let g:neovide_cursor_trail_length=0.8
```

Setting `g:neovide_cursor_trail_length` determines how much the trail of the cursor lags behind the
front edge.

#### Antialiasing

```vim
let g:neovide_cursor_antialiasing=v:true
```

Enables or disables antialiasing of the cursor quad. Disabling may fix some cursor visual issues.

#### Unfocused Outline Width

```vim
let g:neovide_cursor_unfocused_outline_width=0.125
```

Specify cursor outline width in `em`s. You probably want this to be a positive value less than 0.5.
If the value is \<=0 then the cursor will be invisible. This setting takes effect when the editor
window is unfocused, at which time a block cursor will be rendered as an outline instead of as a
full rectangle.

### Cursor Particles

There are a number of vfx modes you can enable which produce particles behind the cursor. These are
enabled by setting `g:neovide_cursor_vfx_mode` to one of the following constants.

#### None at all

```vim
let g:neovide_cursor_vfx_mode = ""
```

The default, no particles at all.

#### Railgun

<img src="./assets/Railgun.gif" alt="Railgun" width=550>

```vim
let g:neovide_cursor_vfx_mode = "railgun"
```

#### Torpedo

<img src="./assets/Torpedo.gif" alt="Torpedo" width=550>

```vim
let g:neovide_cursor_vfx_mode = "torpedo"
```

#### Pixiedust

<img src="./assets/Pixiedust.gif" alt="Pixiedust" width=550>

```vim
let g:neovide_cursor_vfx_mode = "pixiedust"
```

#### Sonic Boom

<img src="./assets/Sonicboom.gif" alt="Sonicboom" width=550>

```vim
let g:neovide_cursor_vfx_mode = "sonicboom"
```

#### Ripple

<img src="./assets/Ripple.gif" alt="Ripple" width=550>

```vim
let g:neovide_cursor_vfx_mode = "ripple"
```

#### Wireframe

<img src="./assets/Wireframe.gif" alt="Wireframe" width=550>

```vim
let g:neovide_cursor_vfx_mode = "wireframe"
```

### Particle Settings

Options for configuring the particle generation and behavior.

#### Particle Opacity

```vim
let g:neovide_cursor_vfx_opacity=200.0
```

Sets the transparency of the generated particles.

#### Particle Lifetime

```vim
let g:neovide_cursor_vfx_particle_lifetime=1.2
```

Sets the amount of time the generated particles should survive.

#### Particle Density

```vim
let g:neovide_cursor_vfx_particle_density=7.0
```

Sets the number of generated particles.

#### Particle Speed

```vim
let g:neovide_cursor_vfx_particle_speed=10.0
```

Sets the speed of particle movement.

#### Particle Phase

```vim
let g:neovide_cursor_vfx_particle_phase=1.5
```

Only for the `railgun` vfx mode.

Sets the mass movement of particles, or how individual each one acts. The higher the value, the less
particles rotate in accordance to each other, the lower, the more line-wise all particles become.

#### Particle Curl

```vim
let g:neovide_cursor_vfx_particle_curl=1.0
```

Only for the `railgun` vfx mode.

Sets the velocity rotation speed of particles. The higher, the less particles actually move and look
more "nervous", the lower, the more it looks like a collapsing sine wave.
