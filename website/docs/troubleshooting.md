# Troubleshooting

- Should Neovide happen not to start at all, check the following:

  - if you're using zsh `~/.zshenv`, `~/.zprofile` and  `~/.zlogin`, or whatever the equivalent for
    your shell is if anything emits output during shell startup, since Neovide uses your shell to
    find `nvim`. You can instead use `~/.zshrc`.

  - whether you can reproduce this by running from the latest git main commit.
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

- If you recieve errors complaining about DRI3 settings, please reference issue
  [#44](https://github.com/neovide/neovide/issues/44#issuecomment-578618052).

- If your scrolling is very stutter

    - Add flag `--novsync` before startup as a quickfix.
    - Check if the value of `g:neovide_refresh_rate` and the refresh rate of your monitor are matched.

    If your `g:neovide_refresh_rate` is correct, then check if you are using dual monitors with mixed refresh rate, say `144` and `60`, by checking output of `xrandr` (wayland should support mixed refresh rate out of the box), if so,that's because X11 does not support mixed refresh rate well and that's not a problem of Neovide. You can find solutions for your setups [here](https://www.reddit.com/r/linux/comments/yaatyo/psa_x11_does_support_mixed_refresh_rate_monitors/), or just set `g:neovide_refresh_rate` to  the lower value.

    As a minimal example, for a window manager without doing any compositing(like dwm) and NVIDIA GPU,

    ```sh
    export __GL_SYNC_DISPLAY_DEVICE=DP-0
    ```

    before startup should do the trick, where `DP-0` is the monitor with higher refresh rate.

