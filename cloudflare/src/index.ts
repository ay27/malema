import contract from "../../leaderboard-contract.json";

interface Statement {
  bind(...args: (string | number)[]): Statement;
  first<T>(): Promise<T | null>;
  run(): Promise<unknown>;
}

interface Env {
  DB: { prepare(sql: string): Statement; batch(statements: Statement[]): Promise<unknown> };
  UPLOAD_LIMIT: { limit(input: { key: string }): Promise<{ success: boolean }> };
  INGEST_MODE?: string;
}

interface Cell {
  harness: string;
  model: string;
  language: "zh" | "en" | "other";
  turns: number;
  hits: number;
}

interface Contribution {
  protocol: number;
  ruleset: string;
  languageVersion: string;
  season: number;
  week: number;
  contributor: string;
  revision: number;
  cells: Cell[];
}

const dayMs = 86_400_000;
const monday = 4;
const maxBytes = 262_144;
const maxTurns = 5_000;
// Harness and model names travel from the client verbatim, so the board can show
// a tool or model the day it ships instead of waiting for a contract release.
// These caps only bound the wire format; they are not allow-lists.
const maxName = 64;
const maxCells = 2_048;
const bucket = (day: number, width: number) => day - ((day - monday) % width);
const today = () => Math.floor(Date.now() / dayMs);
const own = (value: object, key: string) => Object.hasOwn(value, key);
const json = (data: unknown, status = 200, cache = "no-store") =>
  Response.json(data, { status, headers: { "Cache-Control": cache, "X-Content-Type-Options": "nosniff" } });
const fail = (error: string, status: number) => json({ error }, status);

function hasKeys(object: unknown, ...keys: string[]): object is Record<string, unknown> {
  return typeof object === "object" && object !== null && !Array.isArray(object)
    && Object.keys(object).length === keys.length && keys.every(key => own(object, key));
}

function integer(value: unknown, min: number, max: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}

/** A harness or model name: printable, non-empty, already trimmed, bounded length. */
function name(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxName
    && value === value.trim() && !/[\u0000-\u001f\u007f]/.test(value);
}

function accepted(value: unknown, day: number): Contribution | null {
  if (!hasKeys(value, "protocol", "ruleset", "languageVersion", "season", "week", "contributor", "revision", "cells")
    || value.protocol !== contract.protocol || value.ruleset !== contract.ruleset
    || value.languageVersion !== contract.languageVersion
    || !integer(value.season, bucket(day, 28) - 28, bucket(day, 28))
    || !integer(value.week, value.season, Math.min(value.season + 27, day))
    || value.week !== bucket(value.week, 7) || bucket(value.week, 28) !== value.season
    || day >= value.season + 35
    || typeof value.contributor !== "string" || !/^[a-f0-9]{32}$/.test(value.contributor)
    || !integer(value.revision, 1, 1_000_000_000)
    || !Array.isArray(value.cells) || value.cells.length > maxCells) return null;

  const seen = new Set<string>();
  const cells: Cell[] = [];
  for (const candidate of value.cells) {
    if (!hasKeys(candidate, "harness", "model", "language", "turns", "hits")
      || !name(candidate.harness) || !name(candidate.model)
      || (candidate.language !== "zh" && candidate.language !== "en")
      || !integer(candidate.turns, 1, maxTurns) || !integer(candidate.hits, 0, candidate.turns)) return null;
    const { harness, model, language, turns, hits } = candidate;
    const key = [harness, model, language].join("/");
    if (seen.has(key)) return null;
    seen.add(key);
    cells.push({ harness, model, language, turns, hits });
  }
  cells.sort((a, b) => keyOf(a).localeCompare(keyOf(b)));
  return {
    protocol: contract.protocol, ruleset: contract.ruleset, languageVersion: contract.languageVersion,
    season: value.season, week: value.week, contributor: value.contributor, revision: value.revision, cells
  };
}

const keyOf = (cell: Cell) => [cell.harness, cell.model, cell.language].join("/");
const hash = async (value: string) => Array.from(
  new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value))),
  byte => byte.toString(16).padStart(2, "0")
).join("");

async function readJson(request: Request): Promise<unknown> {
  const size = Number(request.headers.get("content-length") ?? 0);
  if (size > maxBytes || !request.body) throw new RangeError();
  const reader = request.body.getReader();
  const payload = new Uint8Array(maxBytes);
  let length = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (length + value.byteLength > maxBytes) {
      await reader.cancel();
      throw new RangeError();
    }
    payload.set(value, length);
    length += value.byteLength;
  }
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(payload.subarray(0, length)));
}

async function intake(request: Request, env: Env, host: string): Promise<Response> {
  if (env.INGEST_MODE === "local") {
    if (host !== "127.0.0.1" && host !== "localhost") return fail("intake_closed", 503);
  } else if (env.INGEST_MODE !== "public") return fail("intake_closed", 503);

  const ip = request.headers.get("cf-connecting-ip");
  if (ip && !(await env.UPLOAD_LIMIT.limit({ key: ip })).success) return fail("rate_limited", 429);
  if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json") {
    return fail("json_required", 415);
  }
  let raw: unknown;
  try { raw = await readJson(request); }
  catch (error) { return error instanceof RangeError ? fail("body_size", 413) : fail("invalid_json", 400); }
  const batch = accepted(raw, today());
  if (!batch) return fail("invalid_contribution", 400);

  const contributor = await hash(batch.contributor);
  const cells = JSON.stringify(batch.cells);
  // One row holds the full weekly contribution, so readers only see complete revisions.
  const written = await env.DB.prepare(`
    INSERT INTO contributions (season, week, contributor, revision, cells)
    VALUES (?1, ?2, ?3, ?4, ?5)
    ON CONFLICT (season, week, contributor) DO UPDATE SET
      revision = excluded.revision, cells = excluded.cells
    WHERE excluded.revision > contributions.revision
    RETURNING revision`
  ).bind(batch.season, batch.week, contributor, batch.revision, cells).first<{ revision: number }>();
  if (written) return json({ status: "accepted", revision: written.revision });
  const existing = await env.DB.prepare(`
    SELECT revision, cells FROM contributions WHERE season = ?1 AND week = ?2 AND contributor = ?3
  `).bind(batch.season, batch.week, contributor).first<{ revision: number; cells: string }>();
  if (existing?.revision === batch.revision && existing.cells === cells) {
    return json({ status: "duplicate", revision: existing.revision });
  }
  if (existing && existing.revision > batch.revision) {
    return json({ status: "stale", revision: existing.revision }, 409);
  }
  return fail("revision_conflict", 409);
}

interface FullContribution {
  contributor: string;
  revision: number;
  cells: Cell[];
  replaces: string[];
}

function acceptedFull(value: unknown): FullContribution | null {
  if (!hasKeys(value, "protocol", "ruleset", "languageVersion", "contributor", "revision", "cells", "replaces")
    || value.protocol !== 3 || value.ruleset !== contract.ruleset || value.languageVersion !== contract.languageVersion
    || typeof value.contributor !== "string" || !/^[a-f0-9]{32}$/.test(value.contributor)
    || !integer(value.revision, 1, 1_000_000_000)
    || !Array.isArray(value.cells) || value.cells.length > maxCells
    || !Array.isArray(value.replaces) || value.replaces.length > 32
    || !value.replaces.every(token => typeof token === "string" && /^[a-f0-9]{32}$/.test(token))) return null;
  const seen = new Set<string>();
  const cells: Cell[] = [];
  for (const cell of value.cells) {
    if (!hasKeys(cell, "harness", "model", "language", "turns", "hits")
      || !name(cell.harness) || !name(cell.model)
      || !["zh", "en", "other"].includes(String(cell.language))
      || !integer(cell.turns, 1, Number.MAX_SAFE_INTEGER) || !integer(cell.hits, 0, cell.turns)) return null;
    const item = cell as unknown as Cell;
    const key = JSON.stringify([item.harness, item.model, item.language]);
    if (seen.has(key)) return null;
    seen.add(key);
    cells.push(item);
  }
  cells.sort((a, b) => keyOf(a).localeCompare(keyOf(b)));
  return { contributor: value.contributor, revision: value.revision, cells, replaces: value.replaces as string[] };
}

async function fullIntake(request: Request, env: Env, host: string): Promise<Response> {
  if (env.INGEST_MODE === "local") {
    if (host !== "127.0.0.1" && host !== "localhost") return fail("intake_closed", 503);
  } else if (env.INGEST_MODE !== "public") return fail("intake_closed", 503);
  const ip = request.headers.get("cf-connecting-ip");
  if (ip && !(await env.UPLOAD_LIMIT.limit({ key: ip })).success) return fail("rate_limited", 429);
  if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json") return fail("json_required", 415);
  let raw: unknown;
  try { raw = await readJson(request); }
  catch (error) { return error instanceof RangeError ? fail("body_size", 413) : fail("invalid_json", 400); }
  const batch = acceptedFull(raw);
  if (!batch) return fail("invalid_contribution", 400);
  const contributor = await hash(batch.contributor);
  const cells = JSON.stringify(batch.cells);
  const statements = [env.DB.prepare(`
    INSERT INTO full_contributions (contributor, revision, cells, updated_at)
    VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (contributor) DO UPDATE SET revision=excluded.revision, cells=excluded.cells, updated_at=excluded.updated_at
    WHERE excluded.revision > full_contributions.revision
  `).bind(contributor, batch.revision, cells, new Date().toISOString())];
  for (const token of new Set(batch.replaces)) {
    statements.push(env.DB.prepare(`
      INSERT OR IGNORE INTO replaced_contributors (legacy_contributor, contributor)
      SELECT ?1, ?2 WHERE EXISTS (
        SELECT 1 FROM full_contributions WHERE contributor=?2 AND revision=?3 AND cells=?4
      )
    `).bind(await hash(token), contributor, batch.revision, cells));
  }
  await env.DB.batch(statements);
  const saved = await env.DB.prepare("SELECT revision,cells FROM full_contributions WHERE contributor=?1")
    .bind(contributor).first<{revision: number; cells: string}>();
  if (saved?.revision === batch.revision && saved.cells === cells) return json({status: "accepted", revision: batch.revision});
  return json({status: saved && saved.revision > batch.revision ? "stale" : "revision_conflict", revision: saved?.revision}, 409);
}

const cumulativeSnapshotSql = `
WITH uploads AS (
  SELECT cells, updated_at FROM full_contributions
  UNION ALL
  SELECT cells, NULL FROM contributions
  WHERE NOT EXISTS (SELECT 1 FROM replaced_contributors WHERE legacy_contributor=contributions.contributor)
  UNION ALL
  SELECT entries, updated_at FROM legacy_archive
), cells AS (
  SELECT json_extract(value, '$.harness') AS harness,
         json_extract(value, '$.model') AS model,
         json_extract(value, '$.language') AS language,
         json_extract(value, '$.turns') AS turns,
         json_extract(value, '$.hits') AS hits
  FROM uploads, json_each(uploads.cells)
), totals AS (
  SELECT harness, model, language, sum(turns) AS turns, sum(hits) AS hits
  FROM cells GROUP BY harness, model, language
)
SELECT (SELECT max(updated_at) FROM uploads) AS updated_at,
       json_group_array(json_object('harness',harness,'model',model,'language',language,'turns',turns,'hits',hits)) AS entries
FROM totals
`;

async function leaderboard(env: Env): Promise<Response> {
  const data = await env.DB.prepare(cumulativeSnapshotSql).first<{updated_at: string | null; entries: string}>();
  return json({ scope: "all", updatedAt: data?.updated_at ?? null, harnesses: contract.harnesses,
    entries: data ? JSON.parse(data.entries) : [] });
}

export default {
  async fetch(request: Request, env: Env, _context: { waitUntil(promise: Promise<unknown>): void }) {
    const url = new URL(request.url);
    try {
      if (url.pathname === "/api/v1/contributions" && request.method === "POST") {
        return await intake(request, env, url.hostname);
      }
      if (url.pathname === "/api/v2/contributions" && request.method === "POST") {
        return await fullIntake(request, env, url.hostname);
      }
      if (url.pathname === "/api/v1/leaderboard" && request.method === "GET") {
        return await leaderboard(env);
      }
      return fail("not_found", 404);
    } catch {
      return fail("temporarily_unavailable", 503);
    }
  },
};
