# tree-sitter-conformant

Tree-sitter grammar and Zed extension for [Plato](https://github.com/conformant-labs/plato2) schemas (`.plato`).

The language's source of truth is the Chevrotain lexer and parser in plato2 (`packages/plato-schema`). This grammar follows those rules closely enough for an editor: it does not report dangling docstrings or check action kinds.

## Develop

```bash
npm install
npm test           # corpus
npm run generate   # regenerate src/ after editing grammar.js
```

Commit the regenerated `src/` with the grammar change.

To check the grammar against every seed schema in a plato2 checkout:

```bash
PLATO_SEEDS=/path/to/plato2/packages/plato-seeds npm run test:seeds
```

## Zed

Syntax highlighting, outline, bracket matching, indentation, folding, diagnostics, completion, hover, go-to-definition, references, rename and formatting. The queries live in `languages/plato`.

Install the language server first (requires Rust):

```bash
cargo install --locked --path crates/plato-language-server
```

The extension finds `plato-language-server` on the worktree's PATH. Alternatively set `lsp.plato.binary.path` in Zed settings to its absolute path. Binary downloads are not configured; rebuild/install the server after changing it or the grammar.

Enable folding ranges for Plato in Zed user or project settings:

```json
{
  "languages": {
    "Plato": { "document_folding_ranges": "on" }
  }
}
```

In Zed, run **zed: install dev extension** and choose this repository. Zed fetches the grammar from GitHub at the commit `extension.toml` names and compiles it, so that commit must be pushed.

To reinstall after changing the queries or extension, run **zed: rebuild dev extension**. Rust extension builds require the `wasm32-wasip2` target (`rustup target add wasm32-wasip2`). If Zed cannot access that toolchain, build with a toolchain that has the target, copy the resulting component, then run **zed: reload extensions**:

```bash
cargo build --locked -p zed-plato --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/zed_plato.wasm extension.wasm
```

This reloads the extension without rebuilding it; the grammar must already be installed.

### Changing the grammar

`extension.toml` pins the grammar to a commit (`rev`). After changing `grammar.js`:

1. Run `npm run generate` and `npm test`, and commit the regenerated `src/`.
2. Push the grammar commit, then set `rev` to its SHA in a follow-up commit so the extension loads matching parser and queries. After a squash merge, update the pin to the commit that landed on `main`.

To try a grammar change before it is pushed, point `repository` at your checkout (`file:///absolute/path/to/tree-sitter-conformant`) and `rev` at a local commit; do not commit that.

These queries use [Zed's capture names](https://zed.dev/docs/extensions/languages). Check a query compiles against the grammar with:

```bash
npx tree-sitter query -p . languages/plato/highlights.scm path/to/schema.plato
```

### Named sections

Group top-level declarations with ordinary comment markers:

```plato
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

The language server makes sections foldable without indenting their contents. It uses the bundled Tree-sitter grammar to supply both region folds and structural folds (models, fields, actions, reads, enums, views, multiline argument lists and docstrings). This matters because Zed uses LSP ranges instead of its normal indentation folds when the server supplies ranges. Closing markers/delimiters remain visible. The Plato runtime treats the markers as comments, so they do not introduce a namespace or change schema behaviour. Without the server and setting above, section folding still requires indentation.

### Semantic features

Diagnostics use the actual Plato parser and schema installer through a persistent Node process. Install Node 24+ and the Plato workspace dependencies. The server finds `packages/plato-schema` in the opened file's ancestors; for standalone schemas, configure it explicitly:

```json
{
  "lsp": {
    "plato": {
      "initialization_options": {
        "schemaRuntimePath": "/absolute/path/to/plato/packages/plato-schema"
      }
    }
  }
}
```

Without that runtime, a warning explains the missing validation; syntax diagnostics, navigation, completion, folding and formatting remain available. Installer errors currently appear at the document start when the runtime supplies no source position.

Each file is an independent schema. Navigation and rename resolve models, roles, enums, fields, actions, inheritance and relationship paths within that file. Completion offers schema symbols, relationship members, enum values and annotations. Hover shows declarations and documentation. Rename edits resolved occurrences, refuses generated fields and name collisions, and validates the result when runtime validation is available. Rename changes schema source only; it does not migrate an installed database.

Formatting normalizes whitespace and indentation while retaining comments, region markers, declaration order, existing line breaks and string contents. It refuses syntactically incomplete documents. Zed's existing format-on-save preference also applies; set `languages.Plato.format_on_save` to `"off"` if formatting should be manual.

### Language server development

```bash
cargo test -p plato-language-server
cargo check -p zed-plato
cargo build -p plato-language-server
PLATO_SCHEMA_RUNTIME=/path/to/plato/packages/plato-schema node --test tests/*.test.mjs
cargo clippy --workspace --all-targets -- -D warnings
```

`crates/plato-language-server` implements stdio LSP with full document synchronization. It compiles the same generated `src/parser.c` as the editor grammar, so grammar updates require rebuilding both. Rust tests cover protocol lifecycle, folding, symbol identity, navigation, rename safety, completion and formatting. The optional Node integration tests exercise runtime diagnostics and rename through LSP, and check that formatting every seed schema preserves its parsed document and is idempotent.
