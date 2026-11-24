import contract from "../../leaderboard-contract.json";

interface Statement {
  bind(...args: (string | number)[]): Statement;
  first<T>(): Promise<T | null>;
  run(): Promise<unknown>;
}

interface Env {
  DB: { prepare(sql: string): Statement };
  UPLOAD_LIMIT: { limit(input: { key: string }): Promise<{ success: boolean }> };
  INGEST_MODE?: string;
  INGEST_INVITE?: string;
}

interface Cell {
  harness: string;
  model: string;
  language: "zh" | "en";
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
const maxBytes = 65_536;
const maxTurns = 5_000;
const maxCells = Object.keys(contract.models).length * Object.keys(contract.harnesses).length * 2;
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
      || typeof candidate.harness !== "string" || !own(contract.harnesses, candidate.harness)
      || typeof candidate.model !== "string" || !own(contract.models, candidate.model)
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
  } else if (env.INGEST_MODE === "invite") {
    const invite = env.INGEST_INVITE;
    const supplied = request.headers.get("authorization") ?? "";
    if (!invite || supplied.length > 256 || !crypto.subtle.timingSafeEqual(
      new TextEncoder().encode(await hash(supplied)),
      new TextEncoder().encode(await hash("Bearer " + invite))
    )) return fail("invite_required", 403);
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

const snapshotSql = `
WITH cells AS (
  SELECT contributor,
    json_extract(value, '$.harness') AS harness,
    json_extract(value, '$.model') AS model,
    json_extract(value, '$.language') AS language,
    json_extract(value, '$.turns') AS turns,
    json_extract(value, '$.hits') AS hits
  FROM contributions, json_each(contributions.cells)
  WHERE season = ?1
),
qualified AS (
  SELECT harness, model, language,
         sum(turns) AS turns, sum(hits) AS hits, count(DISTINCT contributor) AS contributors
  FROM cells
  GROUP BY harness, model, language
  HAVING sum(turns) >= ?2 AND count(DISTINCT contributor) >= ?3
)
INSERT INTO snapshots (season, updated_at, entries)
SELECT ?1, ?4, json_group_array(json_object(
  'harness', harness, 'model', model, 'language', language,
  'turns', turns, 'hits', hits, 'contributors', contributors
))
FROM qualified
WHERE TRUE
ON CONFLICT (season) DO UPDATE SET updated_at = excluded.updated_at, entries = excluded.entries
WHERE excluded.updated_at > snapshots.updated_at
`;

async function publish(env: Env, scheduledTime: number) {
  const day = Math.floor(scheduledTime / dayMs);
  const current = bucket(day, 28);
  const versionTime = new Date(scheduledTime).toISOString();
  await env.DB.prepare(snapshotSql)
    .bind(current, contract.minimumTurns, contract.minimumContributors, versionTime).run();
  // A late-upload window keeps the previous season live for seven days. The
  // first tick after the window closes freezes its final public snapshot.
  const previous = current - 28;
  const last = day - current >= 7
    ? await env.DB.prepare("SELECT updated_at FROM snapshots WHERE season = ?1")
      .bind(previous).first<{ updated_at: string }>()
    : null;
  if (!last || Date.parse(last.updated_at) < (current + 7) * dayMs) {
    await env.DB.prepare(snapshotSql)
      .bind(previous, contract.minimumTurns, contract.minimumContributors, versionTime).run();
  }
  await env.DB.prepare("DELETE FROM contributions WHERE season <= ?1").bind(day - 58).run();
}

async function leaderboard(url: URL, env: Env, local = false): Promise<Response> {
  const period = url.searchParams.get("period") ?? "current";
  if (period !== "current" && period !== "previous") return fail("invalid_period", 400);
  const season = bucket(today(), 28) - (period === "previous" ? 28 : 0);
  const read = () => env.DB.prepare("SELECT updated_at, entries FROM snapshots WHERE season = ?1")
    .bind(season).first<{ updated_at: string; entries: string }>();
  let snapshot = await read();
  if (local && period === "current" && (!snapshot || Date.now() - Date.parse(snapshot.updated_at) >= 600_000)) {
    // Wrangler's local clock only executes Cron on demand. Keep the local
    // preview live while the page is open without exposing a public refresh API.
    await publish(env, Date.now());
    snapshot = await read();
  }
  return json({
    start: new Date(season * dayMs).toISOString().slice(0, 10),
    end: new Date((season + 27) * dayMs).toISOString().slice(0, 10),
    updatedAt: snapshot?.updated_at ?? null,
    thresholds: { turns: contract.minimumTurns, contributors: contract.minimumContributors },
    harnesses: contract.harnesses,
    entries: snapshot ? JSON.parse(snapshot.entries) : []
  }, 200, "public, max-age=60, s-maxage=60");
}

export default {
  async fetch(request: Request, env: Env, context: { waitUntil(promise: Promise<unknown>): void }) {
    const url = new URL(request.url);
    try {
      if (url.pathname === "/api/v1/contributions" && request.method === "POST") {
        return await intake(request, env, url.hostname);
      }
      if (url.pathname === "/api/v1/leaderboard" && request.method === "GET") {
        if (env.INGEST_MODE === "local" && (url.hostname === "127.0.0.1" || url.hostname === "localhost")) {
          return await leaderboard(url, env, true);
        }
        // Cache the two public slices, ignoring arbitrary query strings in the cache key.
        const period = url.searchParams.get("period") ?? "current";
        if (period !== "current" && period !== "previous") return fail("invalid_period", 400);
        const key = new Request(`${url.origin}${url.pathname}?period=${period}`);
        const hit = await caches.default.match(key);
        if (hit) return hit;
        const response = await leaderboard(url, env);
        if (response.ok) context.waitUntil(caches.default.put(key, response.clone()));
        return response;
      }
      return fail("not_found", 404);
    } catch {
      return fail("temporarily_unavailable", 503);
    }
  },
  async scheduled(controller: { scheduledTime: number }, env: Env) {
    await publish(env, controller.scheduledTime);
  }
};
