---@class Args
---@field cmd string

---@type Args
local args = ...

vim.g.cmd = args.cmd

local group_cdpwd = vim.api.nvim_create_augroup("group_cdpwd", { clear = true })
vim.api.nvim_create_autocmd("VimEnter", {
  group = group_cdpwd,
  pattern = "*",
  callback = function()
    vim.api.nvim_command(vim.g.cmd)
  end,
})
