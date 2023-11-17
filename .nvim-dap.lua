local dap = require('dap')

dap.adapters.rust = {
    type = 'server',
    port = "${port}",
    executable = {
        command = 'codelldb.cmd',
        args = { "--port", "${port}" },
    }
}


dap.configurations.rust = {
    {
        name = "debug",
        type = "rust",
        request = "launch",
        program = function()
            return vim.fn.getcwd() .. "/target/debug/neovide.exe"
        end,
        cwd = "${workspaceFolder}",
        stopOnEntry = false,
    },
    {
        name = "release",
        type = "rust",
        request = "launch",
        program = function()
            return vim.fn.getcwd() .. "/target/release/neovide.exe"
        end,
        cwd = "${workspaceFolder}",
        stopOnEntry = false,
    },
}
