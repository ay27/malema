# Leaderboard backend and web board

A TypeScript Worker that stores anonymised weekly counters from the macOS app in
D1 and serves the public board in `public/`. The client and the Worker share
`../leaderboard-contract.json`.

The board is built from `src/web/board.ts`, which imports the arithmetic from
`src/web/logic.ts`. Both are type-checked and the logic module is unit-tested, so
`public/board.js` is a build artifact — edit the TypeScript, not the bundle. The
bundle is committed anyway, so `wrangler deploy` works on a fresh clone without a
build step; `npm run build` refreshes it.

```sh
npm ci
npm run check     # tsc --noEmit
npm test          # node --test, no DOM
npm run build     # esbuild → public/board.js
```

## Local testing

Node.js 22 or newer. From this directory:

```sh
npm ci
npm run migrate:local
npm run dev       # builds the bundle, then wrangler dev on 127.0.0.1:8787
```

The beta desktop app's "Leaderboard" button opens `http://127.0.0.1:8787`.
If something else already owns that port, run the worker elsewhere and point the
app at it with `MALEMA_LEADERBOARD_URL`:

```sh
npx wrangler dev --ip 127.0.0.1 --port 8788 --test-scheduled
MALEMA_LEADERBOARD_URL=http://127.0.0.1:8788 open '/path/to/骂了吗.app'
```

Anonymous submission is off by default; turning it on sends the last seven days
on the first sync. The board stays empty until a snapshot clears the thresholds.

Wrangler refreshes the current snapshot at most every ten minutes while someone
is watching. Trigger the cron by hand to check new data immediately:

```sh
curl 'http://127.0.0.1:8787/__scheduled?cron=*/10+*+*+*+*'
```

## API

- `POST /api/v1/contributions` takes the protocol version, ruleset version,
  season, UTC week and counters. Each week replaces the previous submission only
  when `revision` is higher. Every install can count at most 5,000 messages per
  tool × model × language per week; the client caps the numerator and the
  denominator over the same sample.
- `GET /api/v1/leaderboard?period=current|previous` returns snapshots that
  cleared the threshold. The total only aggregates language groups that passed.

Harness and model names are free-form strings: the Worker checks length and
printable characters instead of an allow-list, so a client that reports a new
tool or an unreleased model still lands in the data. Unrecognised models fall
into the "Other" family on the board instead of being dropped.

## Deploying to Cloudflare

Deploy the site and API together as one Worker with Static Assets and D1. Keep
`wrangler.jsonc` for local development. Production and beta each need their own
Worker name and D1 database.

1. From this directory, sign in with `npx wrangler login` and confirm the
   intended account with `npx wrangler whoami`. The domain must be an active
   Cloudflare zone in that account.
2. Create the production database with `npx wrangler d1 create malema-production`.
   In a separate `wrangler.production.jsonc`, use the production database name
   and ID, keep the `DB` binding and Cron trigger, and set the route to
   `{ "pattern": "malema.app", "custom_domain": true }`. Start with
   `INGEST_MODE=closed` for a read-only launch.
3. From the verified commit, build, migrate, and deploy.

   ```sh
   npm ci
   npm run check
   npm test
   npm run build
   npx wrangler deploy --config wrangler.production.jsonc --dry-run
   npx wrangler d1 migrations apply DB --config wrangler.production.jsonc --remote
   npx wrangler deploy --config wrangler.production.jsonc
   ```

4. Check the site and both API periods on the custom domain. On a fresh
   database the board displays a no-data state. Check that the snapshot
   timestamp advances after the Cron trigger runs.

For beta submissions, set `INGEST_MODE=invite` on the beta Worker and store
`INGEST_INVITE` as a Worker secret. The beta app reads
`MALEMA_LEADERBOARD_INVITE` from its environment. For a public launch,
`INGEST_MODE=public` accepts submissions without an invite. Returning to
`closed` stops new submissions while keeping existing snapshots readable.

Set the repository variable `MALEMA_LEADERBOARD_ENDPOINT` to
`https://malema.app` before building a public desktop release. The beta target
is `https://beta.malema.app`. Local builds default to loopback.
