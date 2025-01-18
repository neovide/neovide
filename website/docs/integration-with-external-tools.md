# Integration w/ External Tools

You can use Neovide in other programs as editor, this page aims to document some quirks. Support for
that, however, is only possible as far as reasonably debuggable.

_Note: We do not endorse nor disrecommend usage of all programs listed here. All usage happens on
your own responsibility._

## [jrnl](https://github.com/jrnl-org/jrnl)

In your configuration file:

```yaml
editor: "neovide"
```

...as `jrnl` saves & removes the temporary file as soon as the main process exits, which happens
before startup by [forking](<https://en.wikipedia.org/wiki/Fork_(system_call)>).

## Quake Mode Accessibility (macOS only)

This feature is quite popular in many terminals aka [iTerm2](https://iterm2.com), [Kitty](https://sw.kovidgoyal.net/kitty/) and [Warp](https://www.warp.dev/f).

At the moment you can achieve the same mode using [Hammerspoon](http://www.hammerspoon.org) just creating key bindings to increase the accessibility and flexibility.

To open Neovide on the current space (with your preferred key-binding) add the following code at `~/.hammerspoon/init.lua`:

```vim
-- Neovide configuration
hs.hotkey.bind({"ctrl", "shift"}, "z", function()
  -- Get current space
  local currentSpace = hs.spaces.focusedSpace()
  -- Get neovide app
  local app = hs.application.get("neovide")
  -- If app already open:
  if app then
    -- If no main window, then open a new window
    if not app:mainWindow() then
      app:selectMenuItem("New OS Window", true)
      -- If app is already in front, then hide it
    elseif app:isFrontmost() then
      app:hide()
      -- If there is a main window somewhere, bring it to current space and to front
    else
      -- First move the main window to the current space
      hs.spaces.moveWindowToSpace(app:mainWindow(), currentSpace)
      -- Activate the app
      app:activate()
      -- Raise the main window and position correctly
      app:mainWindow():raise()
    end
    -- If app not open, open it
  else
    hs.application.launchOrFocus("neovide")
    app = hs.application.get("neovide")
  end
  -- hs.spaces.gotoSpace(currentSpace)
end)
```
