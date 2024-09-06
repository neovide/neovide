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
        cache_enabled = false,
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

---@class neovide.ColorOpacity
---@field disable? boolean
---@field base_opacity? number
---@field multiplier? number
---@field applies_to_foreground? boolean
local neovide_color_opacity = {
    disable = false,
    base_opacity = 0.0,
    multiplier = 1.0,
    applies_to_foreground = true,
}

---Apply opacity to a color
---@param color_index number
---@param opts neovide.ColorOpacity
function _G.neovide_set_transparent_color(color_index, opts)
    if type(opts) == "table" then
        opts = vim.tbl_deep_extend("force", neovide_color_opacity, opts)
        rpcnotify("neovide.set_transparent_color", color_index, opts)
    end
end

vim.api.nvim_exec(
    [[
function! WatchGlobal(variable, callback)
    call dictwatcheradd(g:, a:variable, a:callback)
endfunction
]],
    false
)

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
        end,
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
    end,
})

-- Create auto command for retrieving exit code from neovim on quit.
vim.api.nvim_create_autocmd({ "VimLeavePre" }, {
    pattern = "*",
    once = true,
    nested = true,
    callback = function()
        rpcrequest("neovide.quit", vim.v.exiting)
    end,
})
