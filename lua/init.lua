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

local function take_entity_under_cursor()
  if vim.tbl_contains(vim.g.cursorword_disable_filetypes or {}, vim.bo.filetype) then
    return
  end

  local mouse_pos = vim.fn.getmousepos()
  local guifont = vim.api.nvim_get_option('guifont')
  local column = vim.api.nvim_win_get_cursor(0)[2]
  local cursor = vim.api.nvim_win_get_cursor(0)
  local line = vim.api.nvim_get_current_line()

  -- get the word under the cursor using matchstrpos
  local curword, curword_start, _ = unpack(vim.fn.matchstrpos(line, [[\k*\%]] .. cursor[2] + 1 .. [[c\k*]]))

  -- get screen position of the cursor
  local screenpos = vim.fn.screenpos(mouse_pos.winid, cursor[1], cursor[2] + 1)

  local entity
  local entity_start
  local url_pattern = 'https?://[%w-_%.]+%.%w[%w-_%.%%%?%.:/+=&%%[%]#]*'
  for url in line:gmatch(url_pattern) do
    local s, e = line:find(url, 1, true)
    -- check if the cursor is within this URL
    if column >= s and column <= e then
      entity = url
      entity_start = s - 1
    end
  end

  entity = entity or curword
  entity_start = entity_start or curword_start
  return entity, entity_start + 5, screenpos.row, guifont
end

vim.api.nvim_create_user_command("NeovideForceClick", function()
  local cursorentity, entity_start, entity_end, guifont = take_entity_under_cursor()
  rpcnotify("neovide.force_click", cursorentity, entity_start, entity_end, guifont)
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
