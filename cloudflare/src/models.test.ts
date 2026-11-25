import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import catalog from "../../model-catalog.json" with { type: "json" };
import cases from "../../model-normalization-cases.json" with { type: "json" };
import { normalizeModel, publicCells } from "./models.ts";
import worker from "./index.ts";

test("client and server share the same public-name cases", () => {
  for (const [input, expected] of cases) assert.equal(normalizeModel(input), expected, String(input));
  for (const canonical of Object.keys(catalog.models)) {
    assert.equal(normalizeModel(canonical), canonical);
    assert.equal(normalizeModel(`private-route/${canonical}-sandbox`), canonical);
  }
  assert.equal(normalizeModel("x".repeat(1025)), "other");
});

function database() {
  const db = new DatabaseSync(":memory:");
  for (const name of ["0001_leaderboard.sql", "0002_full_snapshots.sql"]) {
    db.exec(readFileSync(new URL(`../migrations/${name}`, import.meta.url), "utf8"));
  }
  return db;
}
const migration = () => readFileSync(new URL("../migrations/0003_public_model_names.sql", import.meta.url), "utf8");
const cell = (model: string, turns = 10, hits = 1) => ({ harness: "codex", model, language: "zh", turns, hits });

test("migration scrubs all four tables and matches the runtime normalizer", () => {
  const db = database();
  const inputs = cases.filter(([input]) => typeof input === "string").map(([input]) => cell(input!));
  const data = JSON.stringify(inputs);
  db.prepare("INSERT INTO full_contributions VALUES ('installation',1,?,'now')").run(data);
  db.prepare("INSERT INTO contributions VALUES (1,1,'legacy',1,?)").run(data);
  db.prepare("INSERT INTO snapshots VALUES (1,'now',?)").run(data);
  db.prepare("INSERT INTO legacy_archive VALUES (2,'now',?)").run(data);
  db.exec(migration());
  for (const [table, column] of [["full_contributions", "cells"], ["contributions", "cells"], ["snapshots", "entries"], ["legacy_archive", "entries"]]) {
    const row = db.prepare(`SELECT ${column} AS data FROM ${table}`).get()!;
    const values = JSON.parse(row.data as string) as ReturnType<typeof cell>[];
    assert.deepEqual(values.map(c => c.model), inputs.map(c => normalizeModel(c.model)), table);
    assert.equal(values.reduce((n, c) => n + c.turns, 0), inputs.length * 10);
    assert.equal(values.reduce((n, c) => n + c.hits, 0), inputs.length);
  }
  assert.equal(db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE name LIKE '_public_model_%'").get()!.n, 0);
  db.close();
});

class Statement {
  private db: DatabaseSync;
  private sql: string;
  private args: (string | number)[];
  constructor(db: DatabaseSync, sql: string, args: (string | number)[] = []) {
    this.db = db; this.sql = sql; this.args = args;
  }
  bind(...args: (string | number)[]) { return new Statement(this.db, this.sql, args); }
  private parameters() { return Object.fromEntries(this.args.map((value, i) => [`p${i + 1}`, value])); }
  async first<T>(): Promise<T | null> {
    return (this.db.prepare(this.sql.replace(/\?(\d+)/g, ":p$1")).get(this.parameters()) ?? null) as T | null;
  }
  async run() { return this.db.prepare(this.sql.replace(/\?(\d+)/g, ":p$1")).run(this.parameters()); }
}
function env(db: DatabaseSync) {
  return {
    INGEST_MODE: "public",
    UPLOAD_LIMIT: { async limit() { return { success: true }; } },
    DB: {
      prepare(sql: string) { return new Statement(db, sql); },
      async batch(statements: Statement[]) {
        db.exec("BEGIN");
        try { for (const statement of statements) await statement.run(); db.exec("COMMIT"); }
        catch (error) { db.exec("ROLLBACK"); throw error; }
      },
    },
  };
}

test("old clients are sanitized before storage, duplicate receipts survive merging, API remains public-only", async () => {
  const db = database();
  db.exec(migration());
  const environment = env(db);
  const body = {
    protocol: 3, ruleset: "zh-en-4", languageVersion: "zh-en-1", contributor: "a".repeat(32), revision: 1, replaces: [],
    cells: [cell("private-gpt-6.1-sol-sandbox", 20, 2), cell("gpt-6.1-sol", 30, 3), cell("secret-prototype", 40, 4)],
  };
  const post = () => worker.fetch(new Request("https://malema.app/api/v2/contributions", {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
  }), environment, { waitUntil() {} });
  assert.equal((await post()).status, 200);
  assert.equal((await post()).status, 200);
  // A deployed migration may change and merge labels at the same revision.
  db.prepare("UPDATE full_contributions SET cells=?").run(JSON.stringify(body.cells));
  db.exec(migration());
  assert.equal((await post()).status, 200);
  const saved = db.prepare("SELECT cells FROM full_contributions").get()!.cells as string;
  assert.ok(!saved.includes("private") && !saved.includes("secret"));
  assert.deepEqual(publicCells(JSON.parse(saved)), publicCells(body.cells));
  // Read protection covers any pre-deployment or legacy row too.
  db.prepare("INSERT INTO legacy_archive VALUES (1,'now',?)").run(JSON.stringify([cell("private-prototype", 5, 1)]));
  const response = await worker.fetch(new Request("https://malema.app/api/v1/leaderboard"), environment, { waitUntil() {} });
  const result = await response.json() as { entries: ReturnType<typeof cell>[] };
  assert.equal(result.entries.reduce((n, c) => n + c.turns, 0), 95);
  assert.equal(result.entries.reduce((n, c) => n + c.hits, 0), 10);
  assert.ok(result.entries.every(c => c.model === "other" || Object.hasOwn(catalog.models, c.model)));
  db.close();
});

test("legacy weekly uploads are normalized before storage as well", async () => {
  const db = database();
  db.exec(migration());
  const day = Math.floor(Date.now() / 86_400_000);
  const bucket = (width: number) => day - ((day - 4) % width);
  const body = {
    protocol: 2, ruleset: "zh-en-4", languageVersion: "zh-en-1", contributor: "b".repeat(32), revision: 1,
    season: bucket(28), week: bucket(7), cells: [cell("private-glm-5.3-flashx-sandbox"), cell("secret-prototype")],
  };
  for (let i = 0; i < 2; i++) {
    const response = await worker.fetch(new Request("https://malema.app/api/v1/contributions", {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
    }), env(db), { waitUntil() {} });
    assert.equal(response.status, 200);
  }
  const saved = db.prepare("SELECT cells FROM contributions").get()!.cells as string;
  assert.deepEqual(JSON.parse(saved), publicCells(body.cells));
  db.close();
});
