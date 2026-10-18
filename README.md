# tree-sitter-conformant

Tree-sitter grammar for [Plato](https://github.com/conformant-labs/plato2) schemas (`.plato`).

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
