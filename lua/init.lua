---@class Args
---@field neovide_channel_id integer
---@field neovide_version string
---@field register_clipboard boolean
---@field register_right_click boolean
---@field enable_focus_command boolean
---@field global_variable_settings string[]
---@field option_settings string[]

---@type Args
local args = ...


vim.g.neovide_channel_id = args.neovide_channel_id
vim.g.neovide_version = args.neovide_version

-- Set some basic rendering options.
vim.o.lazyredraw = false
vim.o.termguicolors = true

local function rpcnotify(method, ...)
  vim.rpcnotify(vim.g.neovide_channel_id, method, ...)
end

local function rpcrequest(method, ...)
  return vim.rpcrequest(vim.g.neovide_channel_id, method, ...)
end

local function set_clipboard(register)
  return function(lines)
    rpcrequest("neovide.set_clipboard", lines, register)
  end
end

local function get_clipboard(register)
  return function()
    return rpcrequest("neovide.get_clipboard", register)
  end
end

if args.register_clipboard and not vim.g.neovide_no_custom_clipboard then
  vim.g.clipboard = {
    name = "neovide",
    copy = {
      ["+"] = set_clipboard("+"),
      ["*"] = set_clipboard("*"),
    },
    paste = {
      ["+"] = get_clipboard("+"),
      ["*"] = get_clipboard("*"),
    },
    cache_enabled = false
  }
  vim.g.loaded_clipboard_provider = nil
  vim.cmd.runtime("autoload/provider/clipboard.vim")
end



if args.register_right_click then
  vim.api.nvim_create_user_command("NeovideRegisterRightClick", function()
    rpcnotify("neovide.register_right_click")
  end, {})
  vim.api.nvim_create_user_command("NeovideUnregisterRightClick", function()
    rpcnotify("neovide.unregister_right_click")
  end, {})
end

vim.api.nvim_create_user_command("NeovideFocus", function()
  rpcnotify("neovide.focus_window")
end, {})

-- macos trackpad force click
local function matchstr(...)
  local ok, ret = pcall(vim.fn.matchstr, ...)
  return ok and ret or ""
end

local function take_word_under_cursor()
  if vim.tbl_contains(vim.g.cursorword_disable_filetypes or {}, vim.bo.filetype) then
    return
  end

  local column = vim.api.nvim_win_get_cursor(0)[2]
  local line = vim.api.nvim_get_current_line()
  print("Column: " .. column)
  print("Line: " .. line)

  -- Get mouse position
  local mouse_pos = vim.fn.getmousepos()
  print("Mouse Screen Row: " .. mouse_pos.screenrow)
  print("Mouse Screen Column: " .. mouse_pos.screencol)
  print("Mouse Window ID: " .. mouse_pos.winid)
  print("Mouse Window Row: " .. mouse_pos.winrow)
  print("Mouse Window Column: " .. mouse_pos.wincol)
  print("Mouse Line: " .. mouse_pos.line)
  print("Mouse Column: " .. mouse_pos.column)
  print("Mouse Column Offset: " .. mouse_pos.coladd)

  -- Get the word under the cursor
  local left = matchstr(line:sub(1, column + 1), [[\k*$]])
  local right = matchstr(line:sub(column + 1), [[^\k*]]):sub(2)
  local cursorword = left .. right
  print("Cursorword: " .. cursorword)

  -- Get the word under the cursor using matchstrpos
  local cursor = vim.api.nvim_win_get_cursor(0)
  local curword, curword_start, curword_end = unpack(vim.fn.matchstrpos(line, [[\k*\%]] .. cursor[2] + 1 .. [[c\k*]]))
  print("Curword: " .. curword)
  print("Curword start: " .. curword_start)
  print("Curword end: " .. curword_end)

  -- Get screen position of the cursor
  local screenpos = vim.fn.screenpos(mouse_pos.winid, cursor[1], cursor[2] + 1)
  if screenpos then
    print("Screen Row: " .. screenpos.row)
    print("Screen Column: " .. screenpos.col)
  else
    print("Failed to get screen position")
  end

  return curword, curword_start, curword_end
end

vim.api.nvim_create_user_command("NeovideForceClick", function()
  local cursorword, curword_start, curword_end = take_word_under_cursor()
  rpcnotify("neovide.force_click", cursorword, curword_start, curword_end)
end, {})

vim.api.nvim_set_keymap(
  'n',
  '<X1Mouse>',
  "<Cmd>NeovideForceClick<CR>",
  { noremap = true, silent = false }
)

vim.api.nvim_exec([[
function! WatchGlobal(variable, callback)
    call dictwatcheradd(g:, a:variable, a:callback)
endfunction
]], false)

for _, global_variable_setting in ipairs(args.global_variable_settings) do
  local callback = function()
    rpcnotify("setting_changed", global_variable_setting, vim.g["neovide_" .. global_variable_setting])
  end
  vim.fn.WatchGlobal("neovide_" .. global_variable_setting, callback)
end

for _, option_setting in ipairs(args.option_settings) do
  vim.api.nvim_create_autocmd({ "OptionSet" }, {
    pattern = option_setting,
    once = false,
    nested = true,
    callback = function()
      rpcnotify("option_changed", option_setting, vim.o[option_setting])
    end
  })
end

-- Ignore initial values of lines and columns because they are set by neovim directly.
-- See https://github.com/neovide/neovide/issues/2300
vim.api.nvim_create_autocmd({ "VimEnter" }, {
  once = true,
  nested = true,
  callback = function()
    for _, option_setting in ipairs(args.option_settings) do
      if option_setting ~= "lines" and option_setting ~= "columns" then
        rpcnotify("option_changed", option_setting, vim.o[option_setting])
      end
    end
  end
})

-- Create auto command for retrieving exit code from neovim on quit.
vim.api.nvim_create_autocmd({ "VimLeavePre" }, {
  pattern = "*",
  once = true,
  nested = true,
  callback = function()
    rpcrequest("neovide.quit", vim.v.exiting)
  end
})
