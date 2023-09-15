# Troubleshooting

- Should Neovide happen not to start at all, check the following:

  - Shell startup files if they output anything during startup, like `neofetch` or `echo`.
    Neovide uses your shell to find `nvim` and can't know the difference between output and
    `nvim`'s path. You can use your resource file (in the case of zsh `~/.zshrc`) instead for
    such commands.

  - Whether or not you can reproduce this by running from the latest git main commit.
    This can be done by running from source or just grabbing the binary from the [`Actions` tab on
    GitHub](https://github.com/neovide/neovide/actions/workflows/build.yml).

- Neovide requires that a font be set in `init.vim` otherwise errors might be encountered. This can
  be fixed by adding `set guifont=Your\ Font\ Name:h15` in init.vim file. Reference issue
  [#527](https://github.com/neovide/neovide/issues/527).

- If you installed `neovim` via Apple Silicon (M1)-based `brew`, you have to add the `brew prefix`
  to `$PATH` to run `Neovide.app` in GUI. Please see the
  [homebrew documentation](https://docs.brew.sh/FAQ#my-mac-apps-dont-find-homebrew-utilities).
  Reference issue [#1242](https://github.com/neovide/neovide/pull/1242)

## Linux

- If you receive errors complaining about DRI3 settings, please reference issue
  [#44](https://github.com/neovide/neovide/issues/44#issuecomment-578618052).

- If your scrolling is stuttering

  - Add flag `--novsync` before startup as a quickfix.

  - Check if the value of `g:neovide_refresh_rate` and the refresh rate of your monitor are matched.

  - If your `g:neovide_refresh_rate` is correct, then check if you are using dual monitors with
    mixed refresh rate, say `144` and `60`, by checking output of `xrandr` (wayland should support
    mixed refresh rate out of the box), if so,that's because X11 does not support mixed refresh
    rate well. You may be able to fix this through your compositor or by switching to wayland.
    As a temporary work around, you may set `g:neovide_refresh_rate` to the lower value.
