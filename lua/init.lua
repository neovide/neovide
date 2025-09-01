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

local M = {}
M.private = {}

vim.g.neovide_channel_id = args.neovide_channel_id
vim.g.neovide_version = args.neovide_version

-- Set some basic rendering options.
vim.o.lazyredraw = false
vim.o.termguicolors = true

vim.env.NEOVIDE_IMAGE = "1"

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

-- Quit when Command+Q is pressed on macOS
if vim.fn.has("macunix") then
    vim.keymap.set({ "n", "i", "c", "v", "o", "t", "l" }, "<D-q>", function()
        rpcnotify("neovide.exec_detach_handler")
    end)
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

vim.api.nvim_exec2(
    [[
function! WatchGlobal(variable, callback)
    call dictwatcheradd(g:, a:variable, a:callback)
endfunction
]],
    { output = false }
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

M.private.dropfile = function(filename, tabs)
    vim.api.nvim_cmd({
        cmd = "drop",
        args = { vim.fn.fnameescape(filename) },
        -- Always open as the last tabpage
        mods = tabs and { tab = #vim.api.nvim_list_tabpages() } or {},
    }, {})
end

M.disable_redraw = function()
    -- Wrap inside pcall to avoid errors if Neovide disconnects
    pcall(rpcnotify, "neovide.set_redraw", false)
end

M.enable_redraw = function()
    -- Wrap inside pcall to avoid errors if Neovide disconnects
    pcall(rpcnotify, "neovide.set_redraw", true)
end

local point_structure = {
    "x",
    "y",
}

local size_structure = {
    "width",
    "height",
}

local rect_structure = {
    "min",
    "max",
    min = point_structure,
    max = point_structure,
}

local info_structure = {
    "client_area",
    "window_size",
    "cell_size",
    "scale_factor",
    client_area = rect_structure,
    window_size = size_structure,
    cell_size = size_structure,
}

-- The rmpv serialization only supports array, so we need to reconstruct the table structure
local function create_table(structure, value)
    local ret = {}
    for i, field in ipairs(structure) do
        local sub_structure = structure[field]
        if sub_structure then
            ret[field] = create_table(sub_structure, value[i])
        else
            ret[field] = value[i]
        end
    end
    return ret
end

M.private.set_info = function(info)
    M.info = create_table(info_structure, info)
end

M.kitty_image = function(data)
    if not data.a then
        data.a = "t"
    end
    rpcnotify("neovide.kitty_image", data)
end

_G["neovide"] = M
