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

  - Add flags `--no-vsync` and `--no-idle` before startup as a quickfix.

  - Check if the value of `g:neovide_refresh_rate` and the refresh rate of your monitor are matched.

  - If your `g:neovide_refresh_rate` is correct, then check if you are using dual monitors with
    mixed refresh rate, say `144` and `60`, by checking output of `xrandr` (wayland should support
    mixed refresh rate out of the box), if so,that's because X11 does not support mixed refresh
    rate well. You may be able to fix this through your compositor or by switching to wayland.
    As a temporary work around, you may set `g:neovide_refresh_rate` to the lower value.

## Performance Profiling

If you encounter a performance problem like frame rate stuttering, besides attaching a log file
when reporting bugs, [tracy](https://github.com/wolfpld/tracy) profiling data will also be very
useful and can usually help developers to troubleshoot the bug much faster. Here is how you can
collect tracy data.

1. *Install tracy.* Windows users can download it at
[its GitHub release page](https://github.com/wolfpld/tracy/releases). Linux and macOS users can
install it with package manager. Otherwise, you may have to build it yourself following tracy
docs.

2. *Build a profiling version of Neovide.* Follow
[the installation page](https://neovide.dev/installation.html) to install all required
dependencies and Rust SDK. Download or clone
[source code of Neovide](https://github.com/neovide/neovide). Build it with following commands.
Note that you need to specify **both** `--profile profiling` and `--features profiling`, so that
Neovide is built for a profiling version. Or, you can skip these commands, and let `cargo run`
in step 5 build it automatically before running.

    ```sh
    cd [neovide-source-dir]
    cargo build --profile profiling --features profiling
    ```

3. *Prepare tracy for collecting data.* Start tracy with,

    ```sh
    tracy-capture -o [log-file-path]
    ```

    You will see output like this,

    ```plain
    Connecting to 127.0.0.1:8086...
    ```

    It means tracy begins to wait for Neovide and will capture profiling data once it starts.

4. *Running Neovide and reproduce the performance issue.* Start Neovide with following
commands in another terminal. If you have built Neovide with commands in step 3, this should
be very fast. If not, it will build Neovide first. You have to specify
`--profile profiling` and `--features profiling` here, too.

    ```sh
    cd [neovide-source-dir]
    cargo run --profile profiling --features profiling -- --no-fork [neovide-arguments...]
    ```

    Now do whatever leads to performance issue in Neovide and exit.

5. *Get the tracy data and report bugs with it.* Turn to tracy, you will see output like,

    ```plain
    Saving trace... done!
    ```

    You will find tracy log file at the path you specified before. Attach it in your bug
    report! You can also view it yourself with `tracy [log-file-path]`.
