# Commands

On startup, Neovide registers some commands for interacting
with the os and platform window. These are neovim commands
accessible via `:{command name}`.

## Register/Unregister Right Click

On windows you can register a right click context menu item
to edit a given file with Neovide. This can be done at any
time by running the `NeovideRegisterRightClick` command. This can
be undone with the `NeovideUnregisterRightClick` command.

## Focus Window

Running the `NeovideFocus` command will bring the platform
window containing Neovide to the front and activate it. This
is useful for tools like neovim_remote which can manipulate
neovim remotely or if long running tasks would like to
activate the Neovide window after finishing.
