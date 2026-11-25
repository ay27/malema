import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const read = name => JSON.parse(readFileSync(new URL('../../'+name, import.meta.url)));
const catalog = read('model-catalog.json');
const registry = read('model-families.json');
const families = new Set();
for (const family of registry.families) {
 assert.match(family.id, /^[a-z][a-z0-9-]{0,31}$/);
 assert.ok(!families.has(family.id)); families.add(family.id);
 assert.ok(family.name && family.aliases.length && family.sources.length);
 for (const alias of family.aliases) assert.match(alias, /^[a-z0-9][a-z0-9.-]*$/);
 for (const source of family.sources) assert.equal(new URL(source).protocol, 'https:');
}
const aliases = new Map();
for (const [model, names] of Object.entries(catalog.models)) {
 assert.match(model, /^[a-z0-9][a-z0-9._-]{0,63}$/);
 assert.ok(families.has(registry.models[model]), 'Every public model must have a family');
 assert.ok(names.includes(model), 'Every public model must recognize its own name');
 for (const alias of names) {
  assert.match(alias, /^[a-z0-9][a-z0-9._:/-]*$/);
  assert.ok(!aliases.has(alias) || aliases.get(alias)===model, 'One alias must not identify multiple models');
  aliases.set(alias,model);
 }
}
assert.deepEqual(Object.keys(registry.models).sort(), Object.keys(catalog.models).sort());
console.log(`Validated ${families.size} model families, ${Object.keys(catalog.models).length} public models and ${aliases.size} aliases.`);
