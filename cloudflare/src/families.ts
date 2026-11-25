import familyCatalog from "../../model-families.json" with { type: "json" };

export const modelFamilies = familyCatalog.families;
const assignedFamilies: Record<string, string> = familyCatalog.models;
const familyIds = new Set(modelFamilies.map(family => family.id));

export function familyOfModel(value: string): string {
  if (Object.hasOwn(assignedFamilies, value)) return assignedFamilies[value];
  if (value.startsWith("family:") && familyIds.has(value.slice(7))) return value.slice(7);
  const matches: { start: number; end: number; id: string }[] = [];
  for (const family of modelFamilies) for (const alias of family.aliases) {
    let start = value.indexOf(alias);
    while (start >= 0) {
      const end = start + alias.length;
      if ((start === 0 || !/[a-z0-9]/.test(value[start - 1]))
        && (end === value.length || !/[a-z]/.test(value[end]))) matches.push({ start, end, id: family.id });
      start = value.indexOf(alias, start + 1);
    }
  }
  const candidates = new Set(matches.filter(match => !matches.some(other =>
    other.start <= match.start && other.end >= match.end
    && other.end - other.start > match.end - match.start
  )).map(match => match.id));
  return candidates.size === 1 ? [...candidates][0] : "other";
}
