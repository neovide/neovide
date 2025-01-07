# Frequently Asked Questions

Commonly asked questions, or just explanations/elaborations on stuff.

## How can I use cmd-c/cmd-v to copy and paste?

Neovide doesn't add or remove any keybindings to neovim, it only forwards keys. Its likely that
your terminal adds these keybindings, as neovim doesn't have them by default. We can replicate
this behavior by adding keybindings in neovim.

```lua
if vim.g.neovide then
  vim.keymap.set('n', '<D-s>', ':w<CR>') -- Save
  vim.keymap.set('v', '<D-c>', '"+y') -- Copy
  vim.keymap.set('n', '<D-v>', '"+P') -- Paste normal mode
  vim.keymap.set('v', '<D-v>', '"+P') -- Paste visual mode
  vim.keymap.set('c', '<D-v>', '<C-R>+') -- Paste command mode
  vim.keymap.set('i', '<D-v>', '<ESC>l"+Pli') -- Paste insert mode
end

-- Allow clipboard copy paste in neovim
vim.api.nvim_set_keymap('', '<D-v>', '+p<CR>', { noremap = true, silent = true})
vim.api.nvim_set_keymap('!', '<D-v>', '<C-R>+', { noremap = true, silent = true})
vim.api.nvim_set_keymap('t', '<D-v>', '<C-R>+', { noremap = true, silent = true})
vim.api.nvim_set_keymap('v', '<D-v>', '<C-R>+', { noremap = true, silent = true})
```

## How To Enable Floating And Popupmenu Transparency?

Those are controlled through the `winblend` and `pumblend` options. See their help pages for more,
but for short: Both options can be values between `0` (opaque) and `100` (fully transparent),
inclusively on both ends. `winblend` controls the background for floating windows, `pumblend` the
one for the popup menu.

telescope.nvim is different here though. Instead of using the global `winblend` option, it has its
own `telescope.defaults.winblend` configuration option, see [this comment in #1626].

[this comment in #1626]: https://github.com/neovide/neovide/issues/1626#issuecomment-1701080545

## How Can I Dynamically Change The Scale At Runtime?

Neovide offers the setting `g:neovide_scale_factor`, which is multiplied with
the OS scale factor and the font size. So using this could look like

VimScript:

```vim
let g:neovide_scale_factor=1.0
function! ChangeScaleFactor(delta)
  let g:neovide_scale_factor = g:neovide_scale_factor * a:delta
endfunction
nnoremap <expr><C-=> ChangeScaleFactor(1.25)
nnoremap <expr><C--> ChangeScaleFactor(1/1.25)
```

Lua:

```lua
vim.g.neovide_scale_factor = 1.0
local change_scale_factor = function(delta)
  vim.g.neovide_scale_factor = vim.g.neovide_scale_factor * delta
end
vim.keymap.set("n", "<C-=>", function()
  change_scale_factor(1.25)
end)
vim.keymap.set("n", "<C-->", function()
  change_scale_factor(1/1.25)
end)
```

Credits to [BHatGuy here](https://github.com/neovide/neovide/pull/1589).

## How can I Dynamically Change The Transparency At Runtime? (macOS)

VimScript:

```vim
" Set transparency and background color (title bar color)
let g:neovide_transparency=0.0
let g:neovide_transparency_point=0.8
let g:neovide_background_color = '#0f1117'.printf('%x', float2nr(255 * g:neovide_transparency_point))

" Add keybinds to change transparency
function! ChangeTransparency(delta)
  let g:neovide_transparency_point = g:neovide_transparency_point + a:delta
  let g:neovide_background_color = '#0f1117'.printf('%x', float2nr(255 * g:neovide_transparency_point))
endfunction
noremap <expr><D-]> ChangeTransparency(0.01)
noremap <expr><D-[> ChangeTransparency(-0.01)
```

Lua:

```lua
-- Helper function for transparency formatting
local alpha = function()
  return string.format("%x", math.floor(255 * vim.g.neovide_transparency_point or 0.8))
end
-- Set transparency and background color (title bar color)
vim.g.neovide_transparency = 0.0
vim.g.neovide_transparency_point = 0.8
vim.g.neovide_background_color = "#0f1117" .. alpha()
-- Add keybinds to change transparency
local change_transparency = function(delta)
  vim.g.neovide_transparency_point = vim.g.neovide_transparency_point + delta
  vim.g.neovide_background_color = "#0f1117" .. alpha()
end
vim.keymap.set({ "n", "v", "o" }, "<D-]>", function()
  change_transparency(0.01)
end)
vim.keymap.set({ "n", "v", "o" }, "<D-[>", function()
  change_transparency(-0.01)
end)
```

## Neovide Is Not Picking Up Some Shell-configured Information

...aka `nvm use` doesn't work, aka anything configured in `~/.bashrc`/`~/.zshrc`
is ignored by Neovide.

Neovide doesn't start the embedded neovim instance in an interactive shell, so your
shell doesn't read part of its startup file (`~/.bashrc`/`~/.zshrc`/whatever the
equivalent for your shell is). But depending on your shell there are other
options for doing so, for example for zsh you can just put your relevant content
into `~/.zprofile` or `~/.zlogin`.

## The Terminal Displays Fallback Colors/:terminal Does Not Show My Colors

Your colorscheme has to define `g:terminal_color_0` through
`g:terminal_color_15` in order to have any effect on the terminal. Just setting
any random highlights which have `Term` in name won't help.

Some colorschemes think of this, some don't. Search in the documentation of
yours, if it's your own, add it, and if you can't seem to find anything, open an
issue in the colorscheme's repo.

## Compose key sequences do not work

One possible cause might be inconsistent capitalization of your locale
settings, see [#1896]. Possibly you're also running an outdated version of
Neovide.

[#1896]: https://github.com/neovide/neovide/issues/1896#issuecomment-1616421167.

Another possible cause is that you are using IME on X11. Dead keys with IME is
not yet supported, but you can work around that either by disabling IME or
configuring it to only be enabled in insert mode. See
[Configuration](configuration.md).

## Font size is weird with high dpi display on x11

Winit looks in multiple locations for the configured dpi.
Make sure its set in at least one of them. More details
here: [#2010](https://github.com/neovide/neovide/issues/2010#issuecomment-1704416685).

## How to turn off all animations?

Animations can be turned off by setting the following global
variables:

```lua
vim.g.neovide_position_animation_length = 0
vim.g.neovide_cursor_animation_length = 0.00
vim.g.neovide_cursor_trail_size = 0
vim.g.neovide_cursor_animate_in_insert_mode = false
vim.g.neovide_cursor_animate_command_line = false
vim.g.neovide_scroll_animation_far_lines = 0
vim.g.neovide_scroll_animation_length = 0.00
```

## macOS Login Shells

Traditionally, Unix shells use two main configuration files that are executed
before a user can interact with the shell: a profile file and an rc file.

- **Profile File:** This file is typically executed once at login to set up
  the user's environment.
- **RC File:** This file is executed every time a new shell is created to
  configure the shell itself.

In the case of Zsh, which has been the default shell on macOS since version
10.15, the configuration files used are `.zprofile` and `.zshrc`.

### Bash Differences

Unlike Zsh, Bash behaves differently. It only reads `.bashrc` if the shell
session is both interactive and non-login. This distinction might have been
overlooked when macOS transitioned from tcsh to bash in OSX 10.2 Jaguar,
leading developers to place their setup entirely in `.profile` since `.bashrc`
would rarely be executed, especially when starting a new terminal.

With the shift to Zsh as the default shell, both `.zprofile` and `.zshrc` are
executed when starting an interactive non-login shell.

![pic alt](./assets/login-shell.png)

### _Neovide Behavior_

_Regarding to the moment when Neovide launches, it does not start an
interactive shell session, meaning the .bashrc file is not executed. Instead,
the system reads the .bash_profile file. This behavior stems from the
difference in how interactive and login shells process configuration files._

### macOS Specifics

On macOS, the graphical user interface used for system login does not execute
`.zprofile`, as it employs a different method for loading system-level global
settings. This means that terminal emulators must run shells as login shells
to ensure that new shells are properly configured, avoiding potential issues
from missing setup processes in `.zprofile`. This necessity arises because
there is no `.xsession` or equivalent file on macOS to provide initial
settings or global environment variables to terminal sessions[^1].

[^1]: [Why are interactive shells on OSX login shells by default?](https://unix.stackexchange.com/questions/119627/why-are-interactive-shells-on-osx-login-shells-by-default)
