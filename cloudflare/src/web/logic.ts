// Pure helpers behind the public board. Kept free of DOM access so they can be
// type-checked and unit-tested directly.
import { familyOfModel, modelFamilies } from "../families.ts";
export { modelFamilies } from "../families.ts";

export type Language = "zh" | "en";
export type MessageLanguage = Language | "other";

export interface Entry {
  harness: string;
  model: string;
  language: MessageLanguage;
  turns: number;
  hits: number;
}

export interface Snapshot {
  updatedAt: string | null;
  harnesses: Record<string, string>;
  entries: Entry[];
}

/** One tool/model pair with its two language rows merged back together. */
export interface Combination {
  id: string;
  harness: string;
  model: string;
  turns: number;
  hits: number;
  score: number;
  languages: Partial<Record<MessageLanguage, Entry>>;
}

export interface Filters {
  harness: string;
  model: string;
  language: MessageLanguage | "all";
}

export type SortKey = "harness" | "model" | "turns" | "hits" | "score";

export const OTHER_FAMILY = "other";
/** Every stored model and every family fallback use the shared registry. */
export function familyOf(model: string): string {
  return familyOfModel(model.toLowerCase());
}

export function familyName(id: string): string {
  return modelFamilies.find(family => family.id === id)?.name ?? "Other";
}

export const familyOrder = new Map(
  [...modelFamilies.map(family => family.id), OTHER_FAMILY].map((id, index) => [id, index]),
);

const modelNames = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

export function compareModels(a: string, b: string): number {
  const left = familyOrder.get(familyOf(a)) ?? Number.MAX_SAFE_INTEGER;
  const right = familyOrder.get(familyOf(b)) ?? Number.MAX_SAFE_INTEGER;
  return left - right || modelNames.compare(a, b);
}

/** Insults per thousand human messages, the one number the board ranks by. */
export function roastIndex(hits: number, turns: number): number {
  return turns > 0 ? (1000 * hits) / turns : 0;
}

export function combinationId(harness: string, model: string): string {
  return JSON.stringify([harness, model]);
}

export function matches(entry: Entry, filters: Filters): boolean {
  if (filters.harness && entry.harness !== filters.harness) return false;
  if (filters.language !== "all" && entry.language !== filters.language) return false;
  if (filters.model) {
    const [kind, value] = splitModelValue(filters.model);
    if (kind === "model" && entry.model !== value) return false;
    if (kind === "family" && familyOf(entry.model) !== value) return false;
  }
  return true;
}

function splitModelValue(value: string): ["model" | "family" | "", string] {
  if (value.startsWith("model:")) return ["model", value.slice("model:".length)];
  if (value.startsWith("family:")) return ["family", value.slice("family:".length)];
  return ["", value];
}

export function combine(entries: Entry[], filters: Filters): Combination[] {
  const grouped = new Map<string, Combination>();
  for (const entry of entries) {
    if (!matches(entry, filters)) continue;
    const id = combinationId(entry.harness, entry.model);
    let group = grouped.get(id);
    if (!group) {
      group = { id, harness: entry.harness, model: entry.model, turns: 0, hits: 0, score: 0, languages: {} };
      grouped.set(id, group);
    }
    group.hits += entry.hits;
    group.turns += entry.turns;
    group.languages[entry.language] = entry;
  }
  for (const group of grouped.values()) group.score = roastIndex(group.hits, group.turns);
  return [...grouped.values()];
}

export function sortCombinations(
  list: Combination[],
  key: SortKey,
  order: 1 | -1,
  harnessName: (id: string) => string,
): Combination[] {
  const value = (a: Combination, b: Combination): number => {
    if (key === "harness") return harnessName(a.harness).localeCompare(harnessName(b.harness));
    if (key === "model") return compareModels(a.model, b.model);
    return a[key] - b[key];
  };
  return [...list].sort(
    (a, b) => order * value(a, b) || b.turns - a.turns || a.id.localeCompare(b.id),
  );
}

/** 1-based score ranking of the combinations in the current view. */
export function rankByScore(rows: Combination[]): Map<string, number> {
  const ranked = sortCombinations(rows, "score", -1, id => id);
  return new Map(ranked.map((combination, index) => [combination.id, index + 1]));
}

/** A round axis maximum so bar labels stay readable. */
export function axisMaximum(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  for (const step of [1, 1.5, 2, 2.5, 3, 4, 5, 7.5, 10]) {
    const candidate = step * magnitude;
    if (candidate >= value) return candidate;
  }
  return 10 * magnitude;
}

export function axisTicks(maximum: number, count = 4): number[] {
  return Array.from({ length: count + 1 }, (_, index) => (maximum * index) / count);
}

export interface Totals {
  combinations: number;
  turns: number;
  hits: number;
  score: number;
}

export function totals(list: Combination[]): Totals {
  const turns = list.reduce((sum, item) => sum + item.turns, 0);
  const hits = list.reduce((sum, item) => sum + item.hits, 0);
  return { combinations: list.length, turns, hits, score: roastIndex(hits, turns) };
}
