---@class Args
---@field neovide_channel_id integer
---@field neovide_version string
---@field config_path string
---@field register_clipboard boolean
---@field register_right_click boolean
---@field remote boolean
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

vim.api.nvim_create_user_command("NeovideConfig", function()
    if args.remote then
        if vim.fn.filereadable(args.config_path) ~= 0 then
            vim.notify(
                "Neovide is running as a remote server. So the config file may not be on this machine.\n"
                .. "Open it manually if you intend to edit the file anyway.\n"
                .. "Config file location: "
                .. args.config_path,
                vim.log.levels.WARN,
                { title = "Neovide" }
            )
        end
    else
        vim.cmd('edit ' .. vim.fn.fnameescape(args.config_path))
    end
end, {})

local function progress_bar(data)
    -- Wrap inside pcall to avoid errors if Neovide disconnects.
    pcall(rpcnotify, "neovide.progress_bar", data)
end

local function buffer_is_empty(bufnr)
    if vim.api.nvim_buf_line_count(bufnr) ~= 1 then
        return false
    end
    local first_line = vim.api.nvim_buf_get_lines(bufnr, 0, 1, true)[1]
    return first_line == nil or first_line == ""
end

local function current_window_is_only_normal()
    local current = vim.api.nvim_get_current_win()
    local found
    for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
        local config = vim.api.nvim_win_get_config(win)
        if config.relative == "" then
            if found ~= nil then
                return false
            end
            found = win
        end
    end
    return found ~= nil and found == current
end

local function shortmess_allows_intro()
    return not string.find(vim.o.shortmess or "", "I", 1, true)
end

local function should_show_intro_banner()
    local current_buffer = vim.api.nvim_get_current_buf()
    local buffer_name = vim.api.nvim_buf_get_name(current_buffer)
    local current_window = vim.api.nvim_get_current_win()

    return buffer_is_empty(current_buffer)
        and buffer_name == ""
        and current_buffer == 1
        and current_window == 1000
        and current_window_is_only_normal()
        and shortmess_allows_intro()
end

local intro_banner_state

local function notify_intro_state()
    local allowed = false
    local ok, result = pcall(should_show_intro_banner)
    if ok then
        allowed = result
    end

    if intro_banner_state ~= allowed then
        intro_banner_state = allowed
        pcall(rpcnotify, "neovide.intro_banner_allowed", allowed)
    end
end

local function schedule_intro_state_check()
    vim.schedule(notify_intro_state)
end

local intro_group = vim.api.nvim_create_augroup("NeovideIntroBanner", { clear = true })

vim.api.nvim_create_autocmd({
    "VimEnter",
    "BufEnter",
    "BufFilePost",
    "BufNewFile",
    "BufReadPost",
    "TextChanged",
    "TextChangedI",
    "WinEnter",
    "WinNew",
    "WinClosed",
    "TabEnter",
    "TabClosed",
}, {
    group = intro_group,
    callback = schedule_intro_state_check,
})

vim.api.nvim_create_autocmd("OptionSet", {
    group = intro_group,
    pattern = "shortmess",
    callback = schedule_intro_state_check,
})

notify_intro_state()

pcall(vim.api.nvim_create_autocmd, 'Progress', {
    group = vim.api.nvim_create_augroup('NeovideProgressBar', { clear = true }),
    desc = 'Forward progress events to Neovide',
    callback = function(ev)
        if ev.data and ev.data.status == 'running' then
            progress_bar({
                percent = ev.data.percent or 0,
                title = ev.data.title or "",
                message = ev.data.message or "",
            })
        else
            progress_bar({ percent = 100 })
        end
    end,
})

_G["neovide"] = M
