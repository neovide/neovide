---@class ImeContext
---@field entered_preedit_block boolean
---@field base_row integer The absolute position of the cursor's row within the window.
---@field base_col integer The absolute position of the cursor's column within the window.
---@field preedit_col integer

---@class ImePreeditData
---@field preedit_text string

---@class ImeCommitData
---@field commit_text string

---@type ImeContext
local ime_context = {
    entered_preedit_block = false,
    base_col = 0,
    base_row = 0,
    preedit_col = 0,
}

---Getting cursor's row and colomn
---@param window_id? integer if not set, set current window id
---@return integer
---@return integer
local function get_position_under_cursor(window_id)
    local win_id = window_id or vim.api.nvim_get_current_win()
    local under_cursor_position = vim.api.nvim_win_get_cursor(win_id)
    ---@type integer, integer
    local row, col = unpack(under_cursor_position)
    return row, col
end

---@param event vim.api.keyset.create_autocmd.callback_args
local function preedit_handler(event)
    if not ime_context.entered_preedit_block then
        local row, col = get_position_under_cursor()
        ime_context.base_row = row
        ime_context.base_col = col
        ime_context.preedit_col = ime_context.base_col
        ime_context.entered_preedit_block = true
    end
    ---@type ImePreeditData?
    local preedit_data = event.data
    if preedit_data ~= nil then
        vim.api.nvim_buf_set_text(
            0,
            ime_context.base_row - 1,
            ime_context.base_col,
            ime_context.base_row - 1,
            ime_context.preedit_col,
            {}
        )
        ime_context.preedit_col = ime_context.base_col + string.len(preedit_data.preedit_text)
        vim.api.nvim_buf_set_text(
            0,
            ime_context.base_row - 1,
            ime_context.base_col,
            ime_context.base_row - 1,
            ime_context.base_col,
            { preedit_data.preedit_text }
        )
    else
        vim.api.nvim_buf_set_text(
            0,
            ime_context.base_row - 1,
            ime_context.base_col,
            ime_context.base_row - 1,
            ime_context.preedit_col,
            {}
        )
    end
end

---@param event vim.api.keyset.create_autocmd.callback_args
local function commit_handler(event)
    ---@type ImeCommitData
    local commit_data = event.data

    ime_context.preedit_col = ime_context.base_col + string.len(commit_data.commit_text)
    vim.api.nvim_buf_set_text(
        0,
        ime_context.base_row - 1,
        ime_context.base_col,
        ime_context.base_row - 1,
        ime_context.base_col,
        { commit_data.commit_text }
    )
    vim.api.nvim_win_set_cursor(0, { ime_context.base_row, ime_context.preedit_col })

    -- Reset ime context status
    ime_context.base_col, ime_context.base_row = 0, 0
    ime_context.preedit_col = 0
    ime_context.entered_preedit_block = false
end

local ime_au = vim.api.nvim_create_augroup("NeovideImeHandler", { clear = true })

vim.api.nvim_create_autocmd({ "User" }, {
    pattern = "ImePreedit",
    group = ime_au,
    callback = preedit_handler,
})

vim.api.nvim_create_autocmd({ "User" }, {
    pattern = "ImeCommit",
    group = ime_au,
    callback = commit_handler,
})
