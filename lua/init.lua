---@class Args
---@field neovide_channel_id integer
---@field neovide_version string
---@field config_path string
---@field register_clipboard boolean
---@field register_right_click boolean
---@field remote boolean
---@field macos_tab_project_title boolean
---@field enable_focus_command boolean
---@field global_variable_settings string[]
---@field option_settings string[]

---@type Args
local args = ...

local M = {}
M.private = {}

M.show_tab_picker = function()
    vim.schedule(function()
        local tabs = vim.api.nvim_list_tabpages()
        if #tabs == 0 then
            return
        end

        local lines = { "Select a tab number:" }
        for index, tab in ipairs(tabs) do
            local cwd = vim.fn.fnamemodify(vim.fn.getcwd(-1, tab), ":t")
            lines[#lines + 1] = string.format("%d: %s", index, cwd)
        end

        local buf = vim.api.nvim_create_buf(false, true)
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
        vim.api.nvim_buf_set_option(buf, "modifiable", false)
        vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")

        local width = 0
        for _, line in ipairs(lines) do
            if #line > width then
                width = #line
            end
        end
        local height = #lines
        local ui = vim.api.nvim_list_uis()[1] or { width = vim.o.columns, height = vim.o.lines }
        local editor_width = ui.width or vim.o.columns
        local editor_height = ui.height or vim.o.lines
        local row = math.max(0, math.floor((editor_height - height) / 2) - 1)
        local col = math.max(0, math.floor((editor_width - width) / 2))

        local win = vim.api.nvim_open_win(buf, true, {
            relative = "editor",
            row = row,
            col = col,
            width = width,
            height = height,
            style = "minimal",
            border = "rounded",
        })

        vim.cmd("redraw")

        local function close_picker()
            if vim.api.nvim_win_is_valid(win) then
                vim.api.nvim_win_close(win, true)
            end
        end

        local ch = vim.fn.getcharstr()
        close_picker()
        local idx = tonumber(ch)
        if idx and tabs[idx] then
            vim.api.nvim_set_current_tabpage(tabs[idx])
        end
    end)
end

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


if vim.fn.has("mac") == 1 then
    local URL_PATTERN = "https?://[%w-_%.]+%.%w[%w-_%.%%%?%.:/+=&%%[%]#]*"
    local compat_unpack = table.unpack or unpack
    local function find_span(line, cursor_column, pattern, plain)
        local start_pos = 1
        local search_limit = #line + 1
        for _ = 1, search_limit do
            local match_start, match_end = line:find(pattern, start_pos, plain)
            if not match_start then
                break
            end
            if cursor_column >= match_start and cursor_column <= match_end then
                return match_start, match_end
            end
            start_pos = match_end + 1
        end
        return nil, nil
    end

    local function detect_url(line, cursor_column)
        local url_start, url_end = find_span(line, cursor_column, URL_PATTERN, false)
        if not url_start then
            return nil
        end
        return {
            entity = line:sub(url_start, url_end),
            col = url_start - 1,
            kind = "url",
        }
    end

    local function detect_file(line, cursor_column)
        local cfile = vim.fn.expand("<cfile>")
        if not cfile or cfile == "" then
            return nil
        end
        local literal_start = find_span(line, cursor_column, cfile, true)
        if not literal_start then
            return nil
        end
        local absolute_path = vim.fn.fnamemodify(cfile, ":p")
        if absolute_path == "" or not vim.loop.fs_stat(absolute_path) then
            return nil
        end
        return {
            entity = absolute_path,
            col = literal_start - 1,
            kind = "file",
        }
    end

    local function detect_word(line, cursor_column)
        local word, word_col =
            compat_unpack(vim.fn.matchstrpos(line, [[\k*\%]] .. cursor_column .. [[c\k*]]))
        if not word or word == "" then
            return nil
        end
        return {
            entity = word,
            col = word_col,
            kind = "text",
        }
    end

    local ENTITY_DETECTORS = { detect_url, detect_file, detect_word }

    local function detect_entity(line, cursor_col)
        local cursor_column = cursor_col + 1
        for _, detector in ipairs(ENTITY_DETECTORS) do
            local match = detector(line, cursor_column)
            if match then
                return match
            end
        end
        return {
            entity = "",
            col = cursor_col,
            kind = "text",
        }
    end

    local function take_entity_under_cursor()
        local mouse_pos = vim.fn.getmousepos()
        local guifont = vim.api.nvim_get_option("guifont")
        local cursor = vim.api.nvim_win_get_cursor(0)
        local line = vim.api.nvim_get_current_line()

        local match = detect_entity(line, cursor[2])
        local screenpos = vim.fn.screenpos(mouse_pos.winid, cursor[1], match.col + 1)
        local screen_row = math.max(screenpos.row - 1, 0)
        local screen_col = math.max(screenpos.col - 1, 0)

        return screen_col, screen_row, match.entity, guifont, match.kind
    end

    vim.api.nvim_create_user_command("NeovideForceClick", function()
        local col, row, entity, guifont, entity_kind = take_entity_under_cursor()
        rpcnotify("neovide.force_click", col, row, entity, guifont, entity_kind)
    end, {})
end

if vim.fn.has("mac") == 1 and args.macos_tab_project_title then
    local title_value = "%{fnamemodify(getcwd(), ':t')}"
    vim.o.title = true
    vim.o.titlelen = 0
    vim.o.titlestring = title_value

    vim.o.showtabline = 2
    vim.o.tabline = "%!v:lua.NeovideTabline()"

    _G.NeovideTabline = function()
        local tabs = vim.api.nvim_list_tabpages()
        local current = vim.api.nvim_get_current_tabpage()
        local chunks = {}
        for _, tab in ipairs(tabs) do
            local cwd = vim.fn.fnamemodify(vim.fn.getcwd(-1, tab), ":t")
            local hl = (tab == current) and "%#TabLineSel#" or "%#TabLine#"
            table.insert(chunks, hl .. " " .. cwd .. " ")
        end
        return table.concat(chunks) .. "%#TabLineFill#"
    end

    vim.api.nvim_create_autocmd({ "DirChanged", "TabEnter", "VimEnter" }, {
        group = vim.api.nvim_create_augroup("NeovideProjectTitle", { clear = true }),
        callback = function()
            vim.o.titlestring = title_value
        end,
    })
end

if vim.fn.has("mac") == 1 then
    vim.keymap.set("n", "<C-Tab>", ":tabnext<CR>", { silent = true })
    vim.keymap.set("n", "<C-S-Tab>", ":tabprevious<CR>", { silent = true })
end

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

M.private.can_set_background = function()
    local info = vim.api.nvim_get_option_info2("background", {})
    -- Don't change the background if someone else has set it
    if info.was_set and info.last_set_chan ~= args.neovide_channel_id then
        return false
    else
        return true
    end
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
