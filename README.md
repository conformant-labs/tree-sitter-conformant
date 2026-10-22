# tree-sitter-conformant

Tree-sitter grammar, Zed extension and Neovim setup for the schema language used by [Conformant](https://conformant.com) (`.conformant` files, and the older `.plato`).

Conformant's own parser is the language's source of truth. This grammar follows it closely enough for an editor: it does not report dangling docstrings or check action kinds.

## Develop

```bash
npm install
npm test           # corpus
npm run generate   # regenerate src/ after editing grammar.js
```

Commit the regenerated `src/` with the grammar change.

To check the grammar against every `.conformant` and `.plato` file under a directory of schemas (it prints only the files that fail to parse):

```bash
CONFORMANT_SCHEMAS=/path/to/schemas npm run test:schemas
```

## Zed

Syntax highlighting, outline, bracket matching and indentation. The extension lives in `editors/zed`, and its queries in `editors/zed/languages/conformant`.

In Zed, run **zed: install dev extension** and choose `editors/zed`. Zed fetches the grammar from GitHub at the commit `extension.toml` names and compiles it, so that commit must be pushed.

To reinstall after changing the queries, run **zed: rebuild dev extension**.

### Changing the grammar

`editors/zed/extension.toml` pins the grammar to a commit of this repository (`rev`). After changing `grammar.js`:

1. Run `npm run generate` and `npm test`, and commit the regenerated `src/`.
2. Once that commit is on `main` (a squash merge replaces the branch commit, so pin the commit that landed), set `rev` to its SHA.

To try a grammar change before it is pushed, point `repository` at your checkout (`file:///absolute/path/to/tree-sitter-conformant`) and `rev` at a local commit; do not commit that.

These queries use [Zed's capture names](https://zed.dev/docs/extensions/languages). Check a query compiles against the grammar with:

```bash
npx tree-sitter query -p . editors/zed/languages/conformant/highlights.scm path/to/schema.conformant
```

## Neovim

With [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) (the `main` branch) and lazy.nvim, add a spec such as `lua/plugins/conformant.lua`:

```lua
vim.filetype.add({ extension = { conformant = "conformant", plato = "conformant" } })

return {
  "nvim-treesitter/nvim-treesitter",
  init = function()
    vim.api.nvim_create_autocmd("User", {
      pattern = "TSUpdate",
      callback = function()
        require("nvim-treesitter.parsers").conformant = {
          install_info = {
            url = "https://github.com/conformant-labs/tree-sitter-conformant",
            queries = "editors/zed/languages/conformant",
          },
        }
      end,
    })
  end,
  opts = { ensure_installed = { "conformant" } },
}
```

Then run `:TSInstall conformant`, and `:TSInstall! conformant` to update to the latest `main`. Building needs the `tree-sitter` CLI on `PATH` (Mason's `tree-sitter-cli`, or `brew install tree-sitter-cli`).

To work on the grammar, replace `url` with `path = vim.fn.expand("~/git/tree-sitter-conformant")` (your clone). Switch with `:TSUninstall conformant` and then `:TSInstall conformant`, since `:TSInstall!` cannot replace the queries a `url` install copied with a link to your clone. After that, `:TSInstall! conformant` rebuilds after editing the grammar.
