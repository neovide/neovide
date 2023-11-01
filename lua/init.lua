---@class Args
---@field neovide_channel_id integer
---@field register_clipboard boolean
---@field register_right_click boolean
---@field enable_focus_command boolean

---@type Args
local args = ...


vim.g.neovide_channel_id = args.neovide_channel_id

-- Set some basic rendering options.
vim.o.lazyredraw = false
vim.o.termguicolors = true

local function rpcnotify(method, ...)
    vim.rpcnotify(vim.g.neovide_channel_id, method, ...)
end

local function rpcrequest(method, ...)
    vim.rpcrequest(vim.g.neovide_channel_id, method, ...)
end

local function set_clipboard(register)
    return function(lines, regtype)
        rpcrequest("neovide.set_clipboard", lines)
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
        cache_enabled = 0
    }
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

vim.api.nvim_create_autocmd({ "OptionSet" }, {
    pattern = "columns",
    once = false,
    nested = true,
    callback = function()
        rpcnotify("neovide.columns", tonumber(vim.v.option_new))
    end
})

vim.api.nvim_create_autocmd({ "OptionSet" }, {
    pattern = "lines",
    once = false,
    nested = true,
    callback = function()
        rpcnotify("neovide.lines", tonumber(vim.v.option_new))
    end
})

-- Create auto command for retrieving exit code from neovim on quit.
vim.api.nvim_create_autocmd({ "VimLeavePre" }, {
    pattern = "*",
    once = true,
    nested = true,
    callback = function()
        rpcnotify("neovide.quit", vim.v.exiting)
    end
})
