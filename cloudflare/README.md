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

Open `http://127.0.0.1:8787` to test the Worker and the web board directly.
The desktop app uses [malema.app](https://malema.app) for the leaderboard and
anonymous submissions.

Wrangler refreshes the leaderboard at most every ten minutes while someone
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
- `GET /api/v1/leaderboard` returns all-time counts per tool, model and language.
  Archived snapshots retain their counts after the weekly uploads expire.

Harness and model names are free-form strings: the Worker checks length and
printable characters instead of an allow-list, so a client that reports a new
tool or an unreleased model still lands in the data. Unrecognised models fall
into the "Other" family on the board instead of being dropped.

## Deploying to Cloudflare

`wrangler.production.jsonc` deploys the public leaderboard to
[malema.app](https://malema.app).

In **Settings → Secrets and variables → Actions**, add a repository variable
named `CLOUDFLARE_ACCOUNT_ID` and a repository secret named
`CLOUDFLARE_API_TOKEN`. Scope the token to the Cloudflare account and the
`malema.app` zone. The workflow needs permission to deploy Workers, apply D1
migrations, read the zone, and manage Workers routes for the custom domains.

Run **Actions → Deploy Cloudflare → Run workflow** on `main`. The workflow
checks and builds the web bundle, applies pending D1 migrations, deploys the
Worker, and checks the site and the cumulative leaderboard.

To pause submissions, set `INGEST_MODE` to `closed` in
`wrangler.production.jsonc` and redeploy. Existing snapshots stay readable.
