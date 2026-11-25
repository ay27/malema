import assert from "node:assert/strict";
import test from "node:test";
import families from "../../../model-families.json" with { type: "json" };

import {
  axisMaximum,
  axisTicks,
  combine,
  compareModels,
  familyOf,
  matches,
  rankByScore,
  roastIndex,
  sortCombinations,
  totals,
  type Entry,
} from "./logic.ts";

const entry = (
  harness: string,
  model: string,
  language: "zh" | "en",
  turns: number,
  hits: number,
): Entry => ({ harness, model, language, turns, hits });

const all = { harness: "", model: "", language: "all" as const };

test("roast index counts insults per thousand messages", () => {
  assert.equal(roastIndex(0, 100), 0);
  assert.equal(roastIndex(3, 1000), 3);
  // A zero-message group must not divide by zero.
  assert.equal(roastIndex(1, 0), 0);
});

test("unknown model names are grouped instead of dropped", () => {
  // Family detection only sorts entries that the server already sanitized.
  assert.equal(familyOf("gpt-9-omega"), "gpt");
  assert.equal(familyOf("claude-opus-9"), "anthropic");
  assert.equal(familyOf("windsurf-swe-1.7"), "swe");
  assert.equal(familyOf("total-nonsense"), "other");
  assert.equal(familyOf("vendor/model:2026"), "other");
});

test("models sort by family before name", () => {
  const sorted = ["zzz-unknown", "qwen3", "gpt-5.4", "gpt-10.2"].sort(compareModels);
  assert.deepEqual(sorted, ["gpt-5.4", "gpt-10.2", "qwen3", "zzz-unknown"]);
});

test("every public model and unknown-version bucket shares its catalog family", () => {
  for (const [model, family] of Object.entries(families.models)) assert.equal(familyOf(model), family, model);
  for (const family of families.families) assert.equal(familyOf(`family:${family.id}`), family.id);
  for (const [model, family] of [["kimi-k3", "kimi"], ["qwen3-coder", "qwen"], ["minimax-m2.7", "minimax"], ["grok-4.7", "grok"], ["devstral-2512", "mistral"]]) {
    assert.equal(familyOf(model), family);
    const rows = combine([entry("codex", model, "zh", 10, 2)], { ...all, model: `family:${family}` });
    assert.equal(totals(rows).hits, 2);
  }
});

test("combine merges the two languages into one combination", () => {
  const list = combine(
    [entry("codex", "gpt-5.4", "zh", 100, 2), entry("codex", "gpt-5.4", "en", 300, 1)],
    all,
  );
  assert.equal(list.length, 1);
  assert.equal(list[0].turns, 400);
  assert.equal(list[0].hits, 3);
  assert.equal(list[0].score, 7.5);
  assert.equal(list[0].languages.zh?.turns, 100);
  assert.equal(list[0].languages.en?.turns, 300);
});

test("language, harness and model filters narrow the same data", () => {
  const entries = [
    entry("codex", "gpt-5.4", "zh", 100, 2),
    entry("codex", "gpt-5.4", "en", 300, 1),
    entry("claude", "claude-opus-4-5", "zh", 50, 0),
  ];
  assert.equal(combine(entries, { ...all, language: "zh" }).length, 2);
  assert.equal(combine(entries, { ...all, harness: "codex" }).length, 1);
  assert.equal(combine(entries, { ...all, model: "family:gpt" }).length, 1);
  assert.equal(combine(entries, { ...all, model: "model:claude-opus-4-5" }).length, 1);
  assert.equal(combine(entries, { ...all, model: "family:nope" }).length, 0);
  assert.ok(matches(entries[2], { ...all, language: "zh" }));
});

test("filters do not mutate the payload", () => {
  const entries = [entry("codex", "gpt-5.4", "zh", 10, 1)];
  const copy = structuredClone(entries);
  combine(entries, { harness: "claude", model: "family:gpt", language: "en" });
  assert.deepEqual(entries, copy);
});

test("sorting is deterministic and stable for ties", () => {
  const list = combine(
    [
      entry("codex", "gpt-5.4", "zh", 100, 5),
      entry("claude", "claude-opus-4-5", "zh", 100, 5),
      entry("codex", "gpt-5.4", "en", 900, 45),
    ],
    all,
  );
  const byScore = sortCombinations(list, "score", -1, id => id);
  assert.equal(byScore[0].turns, 1000);
  // Families are ordered gpt, anthropic, ... so GPT sorts ahead of Claude.
  const byModel = sortCombinations(list, "model", 1, id => id);
  assert.deepEqual(byModel.map(row => row.model), ["gpt-5.4", "claude-opus-4-5"]);
  const byHarness = sortCombinations(list, "harness", 1, id => (id === "codex" ? "Codex" : "Claude"));
  assert.deepEqual(byHarness.map(row => row.harness), ["claude", "codex"]);
});

test("ranks start at one and follow the cumulative score", () => {
  const entries = [entry("claude", "claude-opus-4-5", "zh", 100, 1), entry("codex", "gpt-5.4", "zh", 100, 5)];
  const ranks = rankByScore(combine(entries, all));
  assert.equal(ranks.size, 2);
  assert.equal(ranks.get(JSON.stringify(["codex", "gpt-5.4"])), 1);
  assert.equal(ranks.get(JSON.stringify(["claude", "claude-opus-4-5"])), 2);
});

test("the language filter ranks the scores displayed in the chart and table", () => {
  const entries = [
    entry("codex", "gpt-5.4", "zh", 320, 15),
    entry("codex", "gpt-5.4", "en", 320, 5),
    entry("claude", "claude-opus-4-5", "zh", 320, 10),
    entry("claude", "claude-opus-4-5", "en", 320, 40),
  ];
  const codex = JSON.stringify(["codex", "gpt-5.4"]);
  assert.equal(rankByScore(combine(entries, all)).get(codex), 2);
  assert.equal(rankByScore(combine(entries, { ...all, language: "zh" })).get(codex), 1);
});

test("axis maxima round up and stay readable", () => {
  assert.equal(axisMaximum(0), 1);
  assert.equal(axisMaximum(3.2), 4);
  assert.equal(axisMaximum(37), 40);
  assert.equal(axisMaximum(960), 1000);
  assert.ok(axisMaximum(1234) >= 1234);
  assert.deepEqual(axisTicks(100, 4), [0, 25, 50, 75, 100]);
});

test("totals add up across a filtered set", () => {
  const list = combine(
    [entry("codex", "gpt-5.4", "zh", 100, 2), entry("codex", "gpt-5.4", "en", 300, 2)],
    all,
  );
  assert.deepEqual(totals(list), { combinations: 1, turns: 400, hits: 4, score: 10 });
});

test("an empty snapshot produces an empty board, not a crash", () => {
  assert.deepEqual(combine([], all), []);
  assert.deepEqual(totals([]), { combinations: 0, turns: 0, hits: 0, score: 0 });
  assert.equal(rankByScore([]).size, 0);
});

test("model rankings sum across harnesses before calculating the rate", () => {
  const entries = [
    entry("codex", "gpt-5.4", "zh", 100, 10),
    entry("cursor", "gpt-5.4", "zh", 900, 0),
    entry("cursor", "gpt-5.4", "en", 1000, 30),
    entry("claude", "claude-opus-4-5", "zh", 100, 3),
  ];
  const original = structuredClone(entries);
  const rows = combine(entries, all, "model");
  assert.equal(rows.length, 2);
  const model = rows.find(row => row.model === "gpt-5.4")!;
  assert.equal(model.harness, "");
  assert.equal(model.turns, 2000);
  assert.equal(model.hits, 40);
  assert.equal(model.score, 20);
  assert.equal(model.languages.zh?.turns, 1000);
  assert.equal(model.languages.zh?.hits, 10);
  assert.equal(rankByScore(rows).get(model.id), 2);
  const zh = combine(entries, { ...all, language: "zh" }, "model");
  assert.equal(zh.find(row => row.model === "gpt-5.4")?.score, 10);
  assert.deepEqual(entries, original);
  assert.equal(totals(rows).turns, totals(combine(entries, all)).turns);
  assert.equal(totals(rows).hits, totals(combine(entries, all)).hits);
});

test("fixed harness and fixed model comparisons apply filters before grouping", () => {
  const entries = [
    entry("codex", "gpt-5.4", "zh", 100, 10),
    entry("codex", "gpt-5.4", "en", 900, 0),
    entry("cursor", "gpt-5.4", "zh", 100, 2),
    entry("cursor", "claude-opus-4-5", "zh", 100, 50),
  ];
  const models = combine(entries, { ...all, harness: "cursor" }, "model");
  assert.equal(models.length, 2);
  assert.equal(totals(models).hits, 52);
  const harnesses = combine(entries, { ...all, model: "model:gpt-5.4" }, "harness");
  assert.equal(harnesses.length, 2);
  assert.deepEqual(sortCombinations(harnesses, "score", -1, id => id).map(row => [row.harness, row.score]), [["cursor", 20], ["codex", 10]]);
  assert.equal(totals(harnesses).hits, 12);
  const zh = combine(entries, { ...all, model: "model:gpt-5.4", language: "zh" }, "harness");
  assert.equal(sortCombinations(zh, "score", -1, id => id)[0].harness, "codex");
  assert.deepEqual(combine(entries, { ...all, harness: "absent" }, "model"), []);
});

test("model groups retain other languages and unknown-model buckets", () => {
  const entries: Entry[] = [
    { harness: "codex", model: "other", language: "other", turns: 10, hits: 1 },
    { harness: "cursor", model: "other", language: "other", turns: 90, hits: 1 },
    entry("cursor", "family:gpt", "zh", 20, 0),
  ];
  const rows = combine(entries, all, "model");
  assert.equal(rows.length, 2);
  const other = rows.find(row => row.model === "other")!;
  assert.equal(other.languages.other?.turns, 100);
  assert.equal(other.score, 20);
  assert.equal(combine(entries, { ...all, model: "family:gpt" }, "model")[0].turns, 20);
  assert.deepEqual(combine([], all, "model"), []);
  assert.deepEqual(combine([], all, "harness"), []);
});
