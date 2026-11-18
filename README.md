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

Syntax highlighting, outline, bracket matching, indentation, folding, diagnostics, completion, hover, go-to-definition, references, rename and formatting. The extension lives in `editors/zed`, and its queries in `editors/zed/languages/conformant`.

Install the language server first (requires Rust):

```bash
cargo install --locked --path crates/plato-language-server
```

The extension finds `plato-language-server` on the worktree's PATH. Alternatively set `lsp.plato.binary.path` in Zed settings to its absolute path. Binary downloads are not configured; rebuild/install the server after changing it or the grammar.

Enable folding ranges for Conformant in Zed user or project settings:

```json
{
  "languages": {
    "Conformant": { "document_folding_ranges": "on" }
  }
}
```

In Zed, run **zed: install dev extension** and choose `editors/zed`. Zed fetches the grammar from GitHub at the commit `extension.toml` names and compiles it, so that commit must be pushed.

To reinstall after changing the queries or extension, run **zed: rebuild dev extension**. Rust extension builds require the `wasm32-wasip2` target (`rustup target add wasm32-wasip2`). If Zed cannot access that toolchain, build with a toolchain that has the target, copy the resulting component, then run **zed: reload extensions**:

```bash
cd editors/zed
cargo build --locked --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/zed_conformant.wasm extension.wasm
```

This reloads the extension without rebuilding it; the grammar must already be installed.

### Changing the grammar

`editors/zed/extension.toml` pins the grammar to a commit of this repository (`rev`). After changing `grammar.js`:

1. Run `npm run generate` and `npm test`, and commit the regenerated `src/`.
2. Push the grammar commit, then set `rev` to its SHA in a follow-up commit so the extension loads matching parser and queries. After a squash merge, update the pin to the commit that landed on `main`.

To try a grammar change before it is pushed, point `repository` at your checkout (`file:///absolute/path/to/tree-sitter-conformant`) and `rev` at a local commit; do not commit that.

These queries use [Zed's capture names](https://zed.dev/docs/extensions/languages). Check a query compiles against the grammar with:

```bash
npx tree-sitter query -p . editors/zed/languages/conformant/highlights.scm path/to/schema.conformant
```

### Named sections

Group top-level declarations with ordinary comment markers:

```conformant
# region Products

model Product {
  fields { name String }
}

# region Views

view AllProducts @layout("table") {
  Product { name }
}

# endregion

# endregion
```

Sections appear by name in Zed's outline and can be nested. Both `# region` and `#region` are accepted; each start needs a name and a matching `# endregion`. Markers wrap complete top-level declarations, not fields inside a model.

The language server makes sections foldable without indenting their contents. It uses the bundled Tree-sitter grammar to supply both region folds and structural folds (models, fields, actions, reads, enums, views, multiline argument lists and docstrings). This matters because Zed uses LSP ranges instead of its normal indentation folds when the server supplies ranges. Closing markers/delimiters remain visible. Conformant's runtime treats the markers as comments, so they do not introduce a namespace or change schema behaviour. Without the server and setting above, section folding still requires indentation.

### Semantic features

Diagnostics use Conformant's own parser and schema installer through a persistent Node process. Install Node 24+ and the conformant repository's workspace dependencies. The server finds `packages/plato-schema` in the opened file's ancestors; for standalone schemas, configure it explicitly:

```json
{
  "lsp": {
    "plato": {
      "initialization_options": {
        "schemaRuntimePath": "/absolute/path/to/conformant/packages/plato-schema"
      }
    }
  }
}
```

Without that runtime, a warning explains the missing validation; syntax diagnostics, navigation, completion, folding and formatting remain available. Installer errors currently appear at the document start when the runtime supplies no source position.

Each file is an independent schema. Navigation and rename resolve models, roles, enums, fields, actions, inheritance and relationship paths within that file. Completion offers schema symbols, relationship members, enum values and annotations. Hover shows declarations and documentation. Rename edits resolved occurrences, refuses generated fields and name collisions, and validates the result when runtime validation is available. Rename changes schema source only; it does not migrate an installed database.

Formatting normalizes whitespace and indentation while retaining comments, region markers, declaration order, existing line breaks and string contents. It refuses syntactically incomplete documents. Zed's existing format-on-save preference also applies; set `languages.Conformant.format_on_save` to `"off"` if formatting should be manual.

### Language server development

```bash
cargo test -p plato-language-server
cargo check --manifest-path editors/zed/Cargo.toml
cargo build -p plato-language-server
PLATO_SCHEMA_RUNTIME=/path/to/conformant/packages/plato-schema node --test tests/*.test.mjs
cargo clippy --workspace --all-targets -- -D warnings
```

`crates/plato-language-server` implements stdio LSP with full document synchronization. It compiles the same generated `src/parser.c` as the editor grammar, so grammar updates require rebuilding both. Rust tests cover protocol lifecycle, folding, symbol identity, navigation, rename safety, completion and formatting. The optional Node integration tests exercise runtime diagnostics and rename through LSP, and check that formatting every seed schema preserves its parsed document and is idempotent.

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

Then run `:TSInstall conformant`.
