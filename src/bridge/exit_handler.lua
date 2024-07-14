local function quit(confirm)
    if confirm then
        vim.cmd("confirm qa")
    else
        vim.cmd("qa!")
    end
end

local function detach_handler(is_remote)
    if is_remote then
        local detach = vim.g.neovide_detach_on_quit or "prompt"
        local c
        if detach == "always_quit" then
            c = 2
        elseif detach == "always_detach" then
            c = 1
        else
            c = vim.fn.confirm("Closing remote connection.", "&Detach\n&Quit\n&Cancel", 1)
        end

        if c == 1 then
            vim.fn.chanclose(vim.g.neovide_channel_id)
        elseif c == 2 then
            quit(vim.g.neovide_confirm_quit or false)
        end
    else
        quit(vim.g.neovide_confirm_quit or false)
    end
end
return detach_handler(...)
