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
