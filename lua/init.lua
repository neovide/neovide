---@class Args
---@field neovide_channel_id integer
---@field register_clipboard boolean
---@field register_right_click boolean
---@field enable_focus_command boolean
---@field global_variable_settings string[]
---@field option_settings string[]

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
    return vim.rpcrequest(vim.g.neovide_channel_id, method, ...)
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

vim.api.nvim_exec([[
function! WatchGlobal(variable, callback)
    call dictwatcheradd(g:, a:variable, a:callback)
endfunction
]], false)

for _,global_variable_setting in ipairs(args.global_variable_settings) do
    local callback = function()
        rpcnotify("setting_changed", global_variable_setting, vim.g["neovide_" .. global_variable_setting])
    end
    vim.fn.WatchGlobal("neovide_" .. global_variable_setting, callback)
end

for _,option_setting in ipairs(args.option_settings) do
    vim.api.nvim_create_autocmd({ "OptionSet" }, {
        pattern = option_setting,
        once = false,
        nested = true,
        callback = function()
            rpcnotify("option_changed", option_setting, vim.o[option_setting])
        end
    })
end

-- Create auto command for retrieving exit code from neovim on quit.
vim.api.nvim_create_autocmd({ "VimLeavePre" }, {
    pattern = "*",
    once = true,
    nested = true,
    callback = function()
        rpcnotify("neovide.quit", vim.v.exiting)
    end
})

local function unlink_highlight(name)
    local highlight = vim.api.nvim_get_hl(0, {name=name, link=false})
    vim.api.nvim_set_hl(0, name, highlight)
end

-- Neovim only reports the final highlight group in the ext_hlstate information
-- So we need to unlink all the groups when the color scheme is changed
-- This is quite hacky, so let the user disable it.
vim.api.nvim_create_autocmd({ "ColorScheme" }, {
    pattern = "*",
    nested = false,
    callback = function()
        if vim.g.neovide_unlink_border_highlights then
            unlink_highlight("FloatTitle")
            unlink_highlight("FloatFooter")
            unlink_highlight("FloatBorder")
            unlink_highlight("WinBar")
            unlink_highlight("WinBarNC")
        end
    end
})
