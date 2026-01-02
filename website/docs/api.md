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

## IME handling

**Requires Neovim Nightly Dec 2 2025 or later.**

`neovide.preedit_handler(
    preedit_raw_text:string,
    cursor_offset:[start_col:integer, end_col:integer]
)`
`neovide.commit_handler(commit_raw_text:string, commit_formatted_text:string)`

These can be used to by your plugin to handle IME events. The pre-edit handler is
called when yourinput method, such as Fcitx, IBus and MS-IME, sends pre-edit event.
So, you have to handle pre-edit texts if you would like to support pre-edit event.
The commit handler is called when your inputmethod sends commit event,
which you decide some text on enabled IME.

In default, `preedit_handler()` is nothing to do and `commit_handler()` uses
[`nvim_input()`](<https://neovim.io/doc/user/api.html#nvim_input()>)

```lua
---@param preedit_raw_text string
---@param cursor_offset_start integer This values show the cursor begin position. The position is byte-wise indexed.
---@param cursor_offset_end integer This values show the cursor end position. The position is byte-wise indexed.
M.preedit_handler = function(preedit_raw_text, cursor_offset_start, cursor_offset_end) end
    -- handle pre-edit event...
end

---@param commit_raw_text string
---@param commit_formatted_text string It's escaped.
neovide.commit_handler = function (commit_raw_text, commit_formatted_text)
    -- handle commit event...
end
```
