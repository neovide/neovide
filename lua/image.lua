local next_id = 1

local backend = {}

local function rpcnotify(method, ...)
    vim.rpcnotify(vim.g.neovide_channel_id, method, ...)
end

---@param image vim.img.Image
---@param opts? vim.img.Backend.RenderOpts
function backend.render(image, opts)
    if image.neovide_id == nil then
        image.neovide_id = next_id
        rpcnotify("neovide.upload_image", next_id, image.data)
        next_id = next_id + 1
    end

    rpcnotify("neovide.show_image", image.neovide_id, opts)
end

vim.img.protocol = function()
    return backend
end
