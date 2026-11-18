import { pathToFileURL } from 'node:url';
import { createInterface } from 'node:readline';
const directory = process.argv[1];
const { parsePlatoWithErrors } = await import(pathToFileURL(`${directory}/src/parse.ts`));
const { applySchemaDocument } = await import(pathToFileURL(`${directory}/src/install.ts`));
const { BUILTIN_FIELD_TYPES } = await import(pathToFileURL(`${directory}/src/schema-type.ts`));
for await (const line of createInterface({ input: process.stdin })) {
  try {
    const { source } = JSON.parse(line);
    const parsed = parsePlatoWithErrors(source);
    const errors = [...parsed.errors];
    let schema;
    if (parsed.doc && !errors.length) {
      try { schema = applySchemaDocument(undefined, parsed.doc); }
      catch (error) { errors.push({ message: error.message }); }
    }
    process.stdout.write(JSON.stringify({ errors, schema, builtinTypes: BUILTIN_FIELD_TYPES }) + '\n');
  } catch (error) {
    process.stdout.write(JSON.stringify({ errors: [{ message: error.message }] }) + '\n');
  }
}
