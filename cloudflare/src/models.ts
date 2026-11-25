import catalog from "../../model-catalog.json" with { type: "json" };

/** Input may contain private routing names. Output is always a public constant. */
export function normalizeModel(raw: unknown): string {
  if (typeof raw !== "string" || raw.length > 1024 || /[^\x20-\x7e]/.test(raw)) return "other";
  const value = raw.trim().toLowerCase().replace(/[_ ]/g, "-");
  if (Object.hasOwn(catalog.models, value)) return value;
  const matches: { start: number; end: number; canonical: string }[] = [];
  for (const [canonical, aliases] of Object.entries(catalog.models)) {
    for (const alias of aliases) {
      let start = value.indexOf(alias);
      while (start >= 0) {
        const end = start + alias.length;
        const before = start === 0 || !/[a-z0-9]/.test(value[start - 1]);
        const after = end === value.length || !/[a-z0-9]/.test(value[end]);
        const longerVersion = /^[.-][0-9]/.test(value.slice(end));
        const dated = /^-(?:[0-9]{8}|[0-9]{4})(?:-|$)/.test(value.slice(end));
        if (before && after && (!longerVersion || dated)) matches.push({ start, end, canonical });
        start = value.indexOf(alias, start + 1);
      }
    }
  }
  const candidates = new Set(matches.filter(match => !matches.some(other =>
    other.start <= match.start && other.end >= match.end
    && other.end - other.start > match.end - match.start
  )).map(match => match.canonical));
  return candidates.size === 1 ? [...candidates][0] : "other";
}

interface Counts { harness: string; model: string; language: string; turns: number; hits: number }
const harnesses = new Set(["codex", "claude", "deepseek", "workbuddy", "cursor", "pi"]);

/** Merge alias collisions without losing any counts. Also bounds the tool label. */
export function publicCells<T extends Counts>(cells: T[]): T[] {
  const grouped = new Map<string, T>();
  for (const cell of cells) {
    const item = { ...cell, harness: harnesses.has(cell.harness) ? cell.harness : "other", model: normalizeModel(cell.model) };
    const key = JSON.stringify([item.harness, item.model, item.language]);
    const previous = grouped.get(key);
    if (previous) { previous.turns += item.turns; previous.hits += item.hits; }
    else grouped.set(key, item);
  }
  return [...grouped.values()].sort((a, b) => JSON.stringify([a.harness, a.model, a.language])
    .localeCompare(JSON.stringify([b.harness, b.model, b.language])));
}
