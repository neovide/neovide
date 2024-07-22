local function matchstr(...)
  local ok, ret = pcall(fn.matchstr, ...)
  return ok and ret or ""
end

local function take_word_under_cursor()
  if vim.tbl_contains(vim.g.cursorword_disable_filetypes or {}, vim.bo.filetype) then
    return
  end

  -- local column = vim.api.nvim_win_get_cursor(0)[2]
  local line = vim.api.nvim_get_current_line()
  -- print("Column: " .. column)
  -- print("Line: " .. line)
  -- local left = matchstr(line:sub(1, column + 1), [[\k*$]])
  -- local right = matchstr(line:sub(column + 1), [[^\k*]]):sub(2)
  --
  -- local cursorword = left .. right
  -- print("Cursorword: " .. cursorword)
  local cursor = vim.api.nvim_win_get_cursor(0)
  local curword, curword_start, curword_end = unpack(vim.fn.matchstrpos(line, [[\k*\%]] .. cursor[2] + 1 .. [[c\k*]]))
  print("Curword: " .. curword)
  print("Curword start: " .. curword_start)
  print("Curword end: " .. curword_end)
  return curword
end

return take_word_under_cursor()
