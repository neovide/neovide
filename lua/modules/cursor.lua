local M = {}

M.take_entity_under_cursor = function()
    local mouse_pos = vim.fn.getmousepos()
    local guifont = vim.api.nvim_get_option("guifont")
    local column = vim.api.nvim_win_get_cursor(0)[2]
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = vim.api.nvim_get_current_line()

    -- get the word under the cursor using matchstrpos
    local curword, curword_start, _ = unpack(vim.fn.matchstrpos(line, [[\k*\%]] .. cursor[2] + 1 .. [[c\k*]]))

    -- get screen position of the cursor
    local screenpos = vim.fn.screenpos(mouse_pos.winid, cursor[1], cursor[2] + 1)

    local entity
    local entity_start
    local url_pattern = "https?://[%w-_%.]+%.%w[%w-_%.%%%?%.:/+=&%%[%]#]*"
    for url in line:gmatch(url_pattern) do
        local s, e = line:find(url, 1, true)
        -- check if the cursor is within this URL
        if column >= s and column <= e then
            entity = url
            entity_start = s - 1
        end
    end

    entity = entity or curword
    entity_start = entity_start or curword_start
    return entity, entity_start + 5, screenpos.row, guifont
end

return M
