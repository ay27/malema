# Leaderboard backend and web board

A TypeScript Worker that stores anonymous full snapshots from the macOS app in
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

```sh
npm ci
npm run migrate:local
npm run dev
```

Open `http://127.0.0.1:8787` to test the Worker and web board. The desktop app
uses [malema.app](https://malema.app).

## API

- `POST /api/v2/contributions` accepts one complete snapshot per installation,
  grouped by tool, model and language. Higher revisions replace the previous
  snapshot. Retrying the same snapshot is safe. An empty snapshot clears that
  installation's counts.
- `GET /api/v1/leaderboard` aggregates the latest installation snapshots on
  every request. The page reloads the counts once a minute.

The v1 contribution endpoint remains available to previously released apps.
Both endpoints normalize model aliases against `../model-catalog.json` before
storage and merge alias collisions. `../model-families.json` also drives the web family filters. Unlisted models in recognized families use fixed `family:<id>` buckets; unknown or conflicting families become `other`.
The public API repeats this normalization when reading legacy data. Migration
`0003_public_model_names.sql` removes original routing labels from all four
business tables while preserving their counts and revisions.

Upgraded installations pass their old anonymous tokens so those weekly counts
are excluded from the total after the full snapshot is received. Finalized
historical snapshots whose source uploads expired are retained in `legacy_archive`.

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
