// Migration 0003 is deployed history. Validate its destinations without rewriting it.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
if (!process.argv.includes('--check')) throw new Error('Migration 0003 is frozen; update the model catalog instead of regenerating deployed SQL.');
const catalog = JSON.parse(readFileSync(new URL('../../model-catalog.json', import.meta.url)));
const sql = readFileSync(new URL('../migrations/0003_public_model_names.sql', import.meta.url), 'utf8');
for (const match of sql.split('CREATE TABLE _public_model_map')[0].matchAll(/\('([^']+)','([^']+)'\)/g)) assert.ok(Object.hasOwn(catalog.models, match[2]), 'Migration target must remain a public constant');
