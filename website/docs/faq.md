# Frequently Asked Questions

Commonly asked questions, or just explanations/elaborations on stuff.

## How To Enable Scrolling Animations and Transparency?

First, [enable multigrid](command-line-reference.md#multigrid), it's not enabled by default.

Then, scrolling animations should work, for transparency see the section below.

## How To Enable Floating And Popupmenu Transparency?

Those are controlled through the `winblend` and `pumblend` options. See their help pages for more,
but for short: Both options can be values between `0` (opaque) and `100` (fully transparent),
inclusively on both ends. `winblend` controls the background for floating windows, `pumblend` the
one for the popup menu.

## How Can I Dynamically Change The Scale At Runtime?

Neovide offers the setting `g:neovide_scale_factor`, which is multiplied with
the OS scale factor and the font size. So using this could look like

```vim
let g:neovide_scale_factor=1.0
function! ChangeScaleFactor(delta)
    let g:neovide_scale_factor = g:neovide_scale_factor * a:delta
endfunction
nnoremap <expr><C-=> ChangeScaleFactor(1.25)
nnoremap <expr><C--> ChangeScaleFactor(1/1.25)
```

Credits to [BHatGuy here](https://github.com/neovide/neovide/pull/1589).

## How can I Dynamically Change The Transparency At Runtime? (macOS)

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
