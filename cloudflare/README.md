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

| Target | Wrangler configuration | Site | Submissions |
| --- | --- | --- | --- |
| Beta | `wrangler.beta.jsonc` | [beta.malema.app](https://beta.malema.app) | Public opt-in |
| Production | `wrangler.production.jsonc` | [malema.app](https://malema.app) | Public opt-in |

The two Workers use separate D1 databases. Use `wrangler.jsonc` for local
development.

In **Settings → Secrets and variables → Actions**, add a repository variable
named `CLOUDFLARE_ACCOUNT_ID` and a repository secret named
`CLOUDFLARE_API_TOKEN`. Scope the token to the Cloudflare account and the
`malema.app` zone. The workflow needs permission to deploy Workers, apply D1
migrations, read the zone, and manage Workers routes for the custom domains.

Open **Actions → Deploy Cloudflare → Run workflow** and choose **beta** or
**production** on `main`. The workflow checks and builds the web bundle, applies
pending migrations to the selected D1, deploys the Worker, and checks the site
and both leaderboard periods.

Both Workers accept contributions that match the app's reporting protocol.
To pause submissions, set `INGEST_MODE` to `closed` in the environment's Wrangler
configuration and redeploy. Existing snapshots stay readable.

Set the repository variable `MALEMA_LEADERBOARD_ENDPOINT` to
`https://malema.app` before building a public desktop release. The beta target
is `https://beta.malema.app`. Local builds default to loopback.
