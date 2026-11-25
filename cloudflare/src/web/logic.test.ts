import assert from "node:assert/strict";
import test from "node:test";

import {
  axisMaximum,
  axisTicks,
  combine,
  compareModels,
  familyOf,
  matches,
  rankByScore,
  rankMovement,
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
  // The board is the client's only allow-list-free surface: a model that ships
  // tomorrow has to render today.
  assert.equal(familyOf("gpt-9-omega"), "gpt");
  assert.equal(familyOf("claude-opus-9"), "anthropic");
  assert.equal(familyOf("windsurf-swe-1.7"), "other");
  assert.equal(familyOf("total-nonsense"), "other");
  assert.equal(familyOf("vendor/model:2026"), "other");
});

test("models sort by family before name", () => {
  const sorted = ["zzz-unknown", "qwen3", "gpt-5.4", "gpt-10.2"].sort(compareModels);
  assert.deepEqual(sorted, ["gpt-5.4", "gpt-10.2", "qwen3", "zzz-unknown"]);
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

test("ranks are 1-based and movement compares two periods", () => {
  const before = [entry("codex", "gpt-5.4", "zh", 100, 1), entry("claude", "claude-opus-4-5", "zh", 100, 5)];
  const after = [entry("codex", "gpt-5.4", "zh", 100, 5), entry("claude", "claude-opus-4-5", "zh", 100, 1)];
  const current = rankByScore(combine(after, all));
  const previous = rankByScore(combine(before, all));
  assert.equal(current.size, 2);
  const movement = rankMovement(current, previous);
  const codex = movement.get(JSON.stringify(["codex", "gpt-5.4"]));
  const claude = movement.get(JSON.stringify(["claude", "claude-opus-4-5"]));
  assert.equal(codex, 1);
  assert.equal(claude, -1);
});

test("a combination with no previous period has no movement", () => {
  const current = rankByScore(combine([entry("codex", "fresh-model", "zh", 10, 1)], all));
  const movement = rankMovement(current, new Map());
  assert.equal(movement.get(JSON.stringify(["codex", "fresh-model"])), null);
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
