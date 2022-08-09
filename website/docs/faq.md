# Frequently Asked Questions

Commonly asked questions, or just explanations/elaborations on stuff.

## How Can I Set The Font/Scale The UI Size?

This is handled through the `guifont` option, settable through Neovim. It's technically documented
in `:h guifont` (type this in Neovim), though some things are misleading there, so here we do what's
discouraged and try to document it ourselves:

- The basic format is `IBM_Plex_Mono,Hack,Noto_Color_Emoji:option1:option2`. You can use arbitrarily
  many "font fallbacks" (`Hack` and `Noto_Color_Emoji` "help out" if `IBM_Plex_Mono` doesn't define
  a character), and arbitrarily many options. Though please note that _first_ all fonts are defined,
  _then_ all options, the options apply "globally".
- Use `:set guifont=*` to open up a window showing what fonts are accessible by Neovide, hit `Enter`
  on one to apply it **temporarily**.
- Spaces in the font name are a bit difficult to write, either use underscores (`_`) or escape them
  (`\`).
- The font options Neovide supports at the moment are:
  - `hXX` — Set the font size to `XX`, can be any (even non-two-digit) number or even a floating
    point number.
  - `b` — Sets the font **bold**.
  - `i` — Sets the font _italic_.

By the way, the default font used is Fira Code at size 14.

## How Can I Dynamically Change The Font Size At Runtime?

Not directly in Neovide, but configurable if you want so. A way to accomplish that in Lua would be:

```lua
vim.g.gui_font_default_size = 12
vim.g.gui_font_size = vim.g.gui_font_default_size
vim.g.gui_font_face = "Fira Code Retina"

RefreshGuiFont = function()
  vim.opt.guifont = string.format("%s:h%s",vim.g.gui_font_face, vim.g.gui_font_size)
end

ResizeGuiFont = function(delta)
  vim.g.gui_font_size = vim.g.gui_font_size + delta
  RefreshGuiFont()
end

ResetGuiFont = function()
  vim.g.gui_font_size = vim.g.gui_font_default_size
  RefreshGuiFont()
end

-- Call function on startup to set default value
ResetGuiFont()

-- Keymaps

local opts = { noremap = true, silent = true }

vim.keymap.set({'n', 'i'}, "<C-+>", function() ResizeGuiFont(1)  end, opts)
vim.keymap.set({'n', 'i'}, "<C-->", function() ResizeGuiFont(-1) end, opts)
```

Credits to [0x0013 here](https://github.com/neovide/neovide/issues/1301#issuecomment-1119370546).

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

Neovide doesn't start the embedded neovim instance in a login shell, so your
shell doesn't read its resource file (`~/.bashrc`/`~/.zshrc`/whatever the
equivalent for your shell is). But depending on your shell there are other
options for doing so, for example for zsh you can just put your relevant content
into `~/.zprofile`.
