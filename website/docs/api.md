# API

The API fuctions are always available without any imports as long as Neovide is connected.

## Redraw Control

`neovide.disable_redraw()`
`neovide.enable_redraw()`

These can be used to by plugins to temporarily disable redrawing while performing some update. They
can for exapmple, be used to prevent the cursor from temporarily moving to the wrong location, or to
atomically spawn a group of windows together. The animations are still updated even when the
re-drawing is disabled, but no new updates from Neovim will be visible.

This is a temporary API, until support for this has been added natively to Neovim.

It's recommended to use the following pattern with `pcall` to ensure that `enable_redraw()` is
always called even when there are errors. And also checking for the existence of the functions.

```lua
if neovide and neovide.disable_redraw then neovide.disable_redraw() end
local success, ret = pcall(actual_function_that_does_something, param1, param2)
if neovide and neovide.enable_redraw then neovide.enable_redraw() end
if success then
    -- do something with the result
else
    -- propagate the error (or ignore it)
    error(ret)
end
```

Or if you don't care about the result

```lua
if neovide and neovide.disable_redraw then neovide.disable_redraw() end
pcall(actual_function_that_does_something, param1, param2)
if neovide and neovide.enable_redraw then neovide.enable_redraw() end
```

**Don't call these functions as a regular user, since you won't see any updates on the screen until
the redrawing is enabled again, so it might be hard to type in the command.**
