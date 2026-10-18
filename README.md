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

Syntax highlighting, outline, bracket matching and indentation. The queries live in `languages/plato`.

In Zed, run **zed: install dev extension** and choose this repository. Zed fetches the grammar from GitHub at the commit `extension.toml` names and compiles it, so that commit must be pushed.

To reinstall after changing the queries, run **zed: rebuild dev extension**.

### Changing the grammar

`extension.toml` pins the grammar to a commit (`rev`). After changing `grammar.js`:

1. Run `npm run generate` and `npm test`, and commit the regenerated `src/`.
2. Once that commit is on `main` (a squash merge replaces the branch commit, so pin the commit that landed), set `rev` to its SHA.

To try a grammar change before it is pushed, point `repository` at your checkout (`file:///absolute/path/to/tree-sitter-conformant`) and `rev` at a local commit; do not commit that.

These queries use [Zed's capture names](https://zed.dev/docs/extensions/languages). Check a query compiles against the grammar with:

```bash
npx tree-sitter query -p . languages/plato/highlights.scm path/to/schema.plato
```
