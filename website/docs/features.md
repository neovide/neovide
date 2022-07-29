# Features

This should be a standard, fully-featured Neovim GUI. Beyond that there are some visual niceties
listed below :)

## Ligatures

Supports ligatures and font shaping.

<img src="./assets/Ligatures.png" alt="Ligatures" width=550>

## Animated Cursor

Cursor animates into position with a smear effect to improve tracking of cursor position.

<img src="./assets/AnimatedCursor.gif" alt="Animated Cursor" width=550>

## Smooth Scrolling

Scroll operations on buffers in neovim will be animated smoothly pixel wise rather than line by line
at a time.

Note: [multigrid](command-line-reference.md#multigrid) must be enabled for this to work.

<img src="./assets/SmoothScrolling.gif" alt="Smooth Scrolling" width=550>

## Animated Windows

Windows animate into position when they are moved making it easier to see how layout changes happen.

Note: [multigrid](command-line-reference.md#multigrid) must be enabled for this to work.

<img src="./assets/AnimatedWindows.gif" alt="Animated Windows" width=550>

## Blurred Floating Windows

The backgrounds of floating windows are blurred improving the visual separation between foreground
and background from built in window transparency.

Note: [multigrid](command-line-reference.md#multigrid) must be enabled for this to work.

<img src="./assets/BlurredFloatingWindows.png" alt="Blurred Floating Windows" width=550>

## Emoji Support

Font fallback supports rendering of emoji not contained in the configured font.

<img src="./assets/Emoji.png" alt="Emojis" width=550>

## WSL Support

Neovide supports displaying a full gui window from inside wsl via the `--wsl` command argument.
Communication is passed via standard io into the wsl copy of neovim providing identical experience
similar to Visual Studio Code's
[Remote Editing](https://code.visualstudio.com/docs/remote/remote-overview).

## Remote TCP Support

Neovide supports connecting to a remote instance of Neovim over a TCP socket via the `--remote-tcp`
command argument. This would allow you to run Neovim on a remote machine and use the GUI on your
local machine, connecting over the network.

Launch Neovim as a TCP server (on port 6666) by running:

```sh
nvim --headless --listen localhost:6666
```

And then connect to it using:

```sh
/path/to/neovide --remote-tcp=localhost:6666
```

By specifying to listen on localhost, you only allow connections from your local computer. If you
are actually doing this over a network you will want to use SSH port forwarding for security, and
then connect as before.

```sh
ssh -L 6666:localhost:6666 ip.of.other.machine nvim --headless --listen localhost:6666
```

Finally, if you would like to leave the neovim server running, close the neovide application window
instead of issuing a `:q` command.

## Some Nonsense ;)

To learn how to configure the following, head on over to the
[configuration](./configuration.md#cursor-particles) section!

### Railgun

<img src="./assets/Railgun.gif" alt="Railgun" width=550>

### Torpedo

<img src="./assets/Torpedo.gif" alt="Torpedo" width=550>

### Pixiedust

<img src="./assets/Pixiedust.gif" alt="Pixiedust" width=550>

### Sonic Boom

<img src="./assets/Sonicboom.gif" alt="Sonicboom" width=550>

### Ripple

<img src="./assets/Ripple.gif" alt="Ripple" width=550>

### Wireframe

<img src="./assets/Wireframe.gif" alt="Wireframe" width=550>
