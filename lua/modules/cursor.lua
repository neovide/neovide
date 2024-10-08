local M = {}

M.take_entity_under_cursor = function()
    local mouse_pos = vim.fn.getmousepos()
    local guifont = vim.api.nvim_get_option("guifont")
    local column = vim.api.nvim_win_get_cursor(0)[2]
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = vim.api.nvim_get_current_line()

    local entity
    local col

    -- get the word under the cursor using matchstrpos
    entity, col, _ = unpack(vim.fn.matchstrpos(line, [[\k*\%]] .. cursor[2] + 1 .. [[c\k*]]))

    -- get screen position of the cursor
    local screenpos = vim.fn.screenpos(mouse_pos.winid, cursor[1], cursor[2] + 1)

    local url_pattern = "https?://[%w-_%.]+%.%w[%w-_%.%%%?%.:/+=&%%[%]#]*"
    for url in line:gmatch(url_pattern) do
        local s, e = line:find(url, 1, true)
        -- check if the cursor is within this URL
        if column >= s and column <= e then
            entity = url
            col = s - 1
        end
    end

    return col + 5, screenpos.row, entity, guifont
end

return M
