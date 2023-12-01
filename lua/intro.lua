local function show_intro(lines)
    -- Create a new unlisted buffer
    local buf = vim.api.nvim_create_buf(false, true)
    local longest_line = 0
    for i, line in ipairs(lines) do
        if line:len() > 0 then
            -- translate
            line = vim.fn.gettext(line)
        end
        longest_line = math.max(longest_line, line:len())
        lines[i] = line
    end

    -- add padding to the lines
    for i, line in ipairs(lines) do
        local padding = math.floor((longest_line - line:len()) / 2) + 1
        lines[i] = string.rep(" ", padding) .. line
    end

    -- add highlight to the special keys
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
    for i, line in ipairs(lines) do
        local s, e = line:find("<.*>")
        if s ~= nil then
            vim.api.nvim_buf_add_highlight(buf, -1, "SpecialKey", i - 1, s - 1, e)
        end
    end

    local width = longest_line + 2
    local height = #lines

    local function get_config()
        return {
            relative = "editor",
            width = width,
            height = height,
            col = math.max(0, math.ceil((vim.o.columns - width) / 2)),
            row = math.max(0, math.ceil((vim.o.lines - height) / 2)),
            style = "minimal",
            border = "none"
        }
    end

    local win = vim.api.nvim_open_win(buf, false, get_config())
    vim.api.nvim_win_set_option(win, "winhighlight", "NormalFloat:Normal")

    local win_autogroup = vim.api.nvim_create_augroup("neovide_intro_win", { clear = true })

    local function close()
        vim.api.nvim_del_augroup_by_id(win_autogroup)
        if vim.api.nvim_buf_is_valid(buf) then
            vim.api.nvim_buf_delete(buf, { force = true })
        end
        if vim.api.nvim_win_is_valid(win) then
            vim.api.nvim_win_close(win, true)
        end
    end

    vim.api.nvim_create_autocmd({ "VimResized" }, {
        pattern = "*",
        group = win_autogroup,
        callback = function()
            vim.api.nvim_win_set_config(win, get_config())
        end
    })

    local first_cursor_move = false

    vim.api.nvim_create_autocmd(
        {
            "BufEnter",
            "ModeChanged",
            "CursorMoved", "CursorMovedI",
            "TextChanged", "TextChangedI", "TextChangedP", "TextChangedT"
        },
        {
            pattern = "*",
            group = win_autogroup,
            callback = function(param)
                -- Allow changing from and to command mode
                if param.event == "ModeChanged" and param.match:find("c") ~= nil then
                    return
                end
                -- The first cursor move is ignored, since it happens after the message is shown
                if not first_cursor_move and param.event == "CursorMoved" then
                    first_cursor_move = true
                    return
                end
                close()
            end
        })
end

-- Only used for Nvim 0.9.2, nvim sends the message on newer versions
local function get_intro_lines()
    local version_string = vim.api.nvim_exec2("version", { output = true }).output
    -- we only need the first line of the version
    version_string = version_string:sub(2, version_string:find("\n", 2) - 1)
    local version = vim.version()

    local lines = {
        version_string,
        "",
        "Nvim is open source and freely distributable",
        "https://neovim.io/#chat",
        "",
        "type  :help nvim<Enter>       if you are new! ",
        "type  :checkhealth<Enter>     to optimize Nvim",
        "type  :q<Enter>               to exit         ",
        "type  :help<Enter>            for help        ",
        "",
        string.format("type  :help news<Enter> to see changes in v%d.%d", version.major, version.minor),
        "",
    }

    local help_messages = {
        {
            "Sponsor Vim development!",
            "type  :help sponsor<Enter>    for information ",
        },
        {
            "Become a registered Vim user!",
            "type  :help register<Enter>   for information ",
        },
        {
            "Help poor children in Uganda!",
            "type  :help iccf<Enter>       for information ",
        },
    }

    -- Show the sponsor and register message one out of four times, the Uganda
    -- message two out of four times.
    -- Use Vim rather than lua for rand to avoid polluting the random seed
    local seed = vim.fn.srand()
    local help_type = (vim.fn.rand(seed) % 4) + 1
    if help_type <= 2 then
        vim.list_extend(lines, help_messages[help_type])
    else
        vim.list_extend(lines, help_messages[3])
    end
    return lines
end


local function setup_autocommand()
    local autogroup = vim.api.nvim_create_augroup("neovide_intro", { clear = true })
    vim.api.nvim_create_autocmd({ "VimEnter" }, {
        pattern = "*",
        group = autogroup,
        once = true,
        callback = function()
            -- Don't show when disabled
            if vim.o.shortmess:find("I") then
                return
            end

            -- Don't show the intro if a buffer is loaded
            if vim.api.nvim_buf_line_count(0) > 1 or vim.api.nvim_buf_get_name(0) ~= "" then
                return
            end

            show_intro(get_intro_lines())
        end
    })
end


local function entry(function_name, ...)
    local api_metadata = vim.fn.api_info()
    local has_msg_intro = vim.tbl_contains(api_metadata.ui_events, function(v)
        return v.name == "msg_intro"
    end, { predicate = true })

    if has_msg_intro then
        if function_name == "show_intro" then
            show_intro({...})
        end
    elseif function_name == "setup_autocommand" then
        setup_autocommand()
    end
end

entry(...)
