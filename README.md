# tree-sitter-conformant

Tree-sitter grammar, Zed extension and Neovim setup for [Conformant](https://github.com/conformant-labs/conformant) schemas (`.conformant`, and the older `.plato`).

The language's source of truth is the Chevrotain lexer and parser in conformant (`packages/plato-schema`). This grammar follows those rules closely enough for an editor: it does not report dangling docstrings or check action kinds.

## Develop

```bash
npm install
npm test           # corpus
npm run generate   # regenerate src/ after editing grammar.js
```

Commit the regenerated `src/` with the grammar change.

To check the grammar against every seed schema in a conformant checkout:

```bash
CONFORMANT_SEEDS=/path/to/conformant/packages/plato-seeds npm run test:seeds
```

## Zed

Syntax highlighting, outline, bracket matching and indentation. The queries live in `languages/conformant`.

In Zed, run **zed: install dev extension** and choose this repository. Zed fetches the grammar from GitHub at the commit `extension.toml` names and compiles it, so that commit must be pushed.

To reinstall after changing the queries, run **zed: rebuild dev extension**.

### Changing the grammar

`extension.toml` pins the grammar to a commit (`rev`). After changing `grammar.js`:

1. Run `npm run generate` and `npm test`, and commit the regenerated `src/`.
2. Once that commit is on `main` (a squash merge replaces the branch commit, so pin the commit that landed), set `rev` to its SHA.

To try a grammar change before it is pushed, point `repository` at your checkout (`file:///absolute/path/to/tree-sitter-conformant`) and `rev` at a local commit; do not commit that.

These queries use [Zed's capture names](https://zed.dev/docs/extensions/languages). Check a query compiles against the grammar with:

```bash
npx tree-sitter query -p . languages/conformant/highlights.scm path/to/schema.conformant
```

## Neovim

With [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) (the `main` branch) and lazy.nvim, add a spec such as `lua/plugins/conformant.lua`, pointing `path` at your clone of this repository:

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
            path = vim.fn.expand("~/git/tree-sitter-conformant"),
            queries = "languages/conformant",
          },
        }
      end,
    })
  end,
  opts = { ensure_installed = { "conformant" } },
}
```

Then run `:TSInstall conformant`, and `:TSInstall! conformant` to rebuild after pulling or editing the grammar. Building needs the `tree-sitter` CLI on `PATH` (Mason's `tree-sitter-cli`, or `brew install tree-sitter-cli`).

nvim-treesitter downloads a `url` source as a GitHub archive without credentials, so it cannot install from this repository while it is private.
