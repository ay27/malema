// Public leaderboard front-end. Rendering lives here; every number comes from
// ./logic.ts so the arithmetic is unit-tested without a DOM.
import {
  axisMaximum,
  axisTicks,
  combine,
  compareModels,
  familyName,
  familyOf,
  OTHER_FAMILY,
  rankByScore,
  roastIndex,
  sortCombinations,
  totals,
  type Combination,
  type Entry,
  type Language,
  type MessageLanguage,
  type Snapshot,
  type SortKey,
} from "./logic";

type Copy = {
  brand: string;
  title: string;
  tagline: string;
  messageLanguage: string;
  allLanguages: string;
  harness: string;
  allHarnesses: string;
  model: string;
  allModels: string;
  modelFamilies: string;
  otherModels: string;
  family: (name: string) => string;
  compare: string;
  details: string;
  rank: string;
  turns: string;
  hits: string;
  score: string;
  units: string;
  combinations: string;
  messages: string;
  insults: string;
  overall: string;
  top: string;
  skip: string;
  navigation: string;
  filters: string;
  summary: string;
  retry: string;
  loading: string;
  noData: string;
  noDataDetails: string;
  noMatch: string;
  noMatchDetails: string;
  failed: string;
  failedDetails: string;
  topSix: string;
  updated: string;
  noSnapshot: string;
  count: (n: string) => string;
  ofTotal: (shown: string, all: string) => string;
  otherUI: string;
  zhLabel: string;
  enLabel: string;
  otherLabel: string;
};

const translations: Record<Language, Copy> = {
  zh: {
    brand: "骂了吗",
    title: "全球挨骂榜",
    tagline: "每千条真人消息里，AI 明确挨骂几次。",
    messageLanguage: "消息语言",
    allLanguages: "全部",
    harness: "运行工具",
    allHarnesses: "全部工具",
    model: "回复模型",
    allModels: "全部模型",
    modelFamilies: "模型系列",
    otherModels: "其他模型",
    family: name => name + " 系列",
    compare: "组合对比",
    details: "组合明细",
    rank: "名次",
    turns: "真人消息",
    hits: "明确挨骂",
    score: "挨骂指数",
    units: "每千条消息的明确挨骂数",
    combinations: "组合",
    messages: "累计真人消息",
    insults: "累计挨骂",
    overall: "整体指数",
    top: "回到顶部 ↑",
    skip: "跳到榜单",
    navigation: "导航",
    filters: "榜单筛选",
    summary: "汇总",
    retry: "再试一次",
    loading: "正在读取榜单",
    noData: "榜单还没有数据",
    noDataDetails: "在 App 里开启匿名提交，计数会随榜单更新显示。",
    noMatch: "没有匹配的组合",
    noMatchDetails: "试试切换语言、模型或运行工具。",
    failed: "暂时读不到榜单",
    failedDetails: "请稍后重试。",
    topSix: "前 6 组 · ",
    updated: "快照更新于 ",
    noSnapshot: "等待首份榜单快照",
    count: n => `${n} 个组合`,
    ofTotal: (shown, all) => `显示 ${shown} / ${all}`,
    otherUI: "Switch interface to English",
    zhLabel: "中文",
    enLabel: "English",
    otherLabel: "其他",
  },
  en: {
    brand: "Malema",
    title: "Global AI Roasting Board",
    tagline: "How often an AI gets explicitly insulted, per thousand human messages.",
    messageLanguage: "Message language",
    allLanguages: "All",
    harness: "Harness",
    allHarnesses: "All harnesses",
    model: "Reply model",
    allModels: "All models",
    modelFamilies: "Model families",
    otherModels: "Other models",
    family: name => name + " family",
    compare: "Top combinations",
    details: "All combinations",
    rank: "#",
    turns: "Human messages",
    hits: "Clear insults",
    score: "Roast index",
    units: "clear insults per 1,000 messages",
    combinations: "Combinations",
    messages: "Total human messages",
    insults: "Total clear insults",
    overall: "Overall index",
    top: "Back to top ↑",
    skip: "Skip to leaderboard",
    navigation: "Navigation",
    filters: "Leaderboard filters",
    summary: "Summary",
    retry: "Try again",
    loading: "Loading rankings",
    noData: "No leaderboard data yet",
    noDataDetails: "Enable Anonymous in the app to share counts for the next leaderboard update.",
    noMatch: "No matching combinations",
    noMatchDetails: "Try another language, model, or harness.",
    failed: "Could not load rankings",
    failedDetails: "Please try again shortly.",
    topSix: "Top 6 · ",
    updated: "Snapshot updated ",
    noSnapshot: "Waiting for the first snapshot",
    count: n => `${n} combinations`,
    ofTotal: (shown, all) => `showing ${shown} of ${all}`,
    otherUI: "切换至中文界面",
    zhLabel: "中文",
    enLabel: "English",
    otherLabel: "Other",
  },
};

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;

function dom<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

interface Ui {
  locale: Language;
  filtering: MessageLanguage | "all";
  key: SortKey;
  order: 1 | -1;
  state: "loading" | "ready" | "error";
  data: Snapshot | null;
}

const languagePreference = "malema-interface-language";

function initialLanguage(): Language {
  try {
    const saved = localStorage.getItem(languagePreference);
    if (saved === "en" || saved === "zh") return saved;
  } catch {
    // The switch still works for this visit when browser storage is unavailable.
  }
  return document.documentElement.lang.startsWith("en") ? "en" : "zh";
}

const ui: Ui = {
  locale: initialLanguage(),
  filtering: "all",
  key: "score",
  order: -1,
  state: "loading",
  data: null,
};

let activeRequest: AbortController | undefined;

const copy = (): Copy => translations[ui.locale];
function modelLabel(model: string): string {
  if (model === "other") return copy().otherModels;
  if (model.startsWith("family:")) return familyName(familyOf(model)) + " · " + copy().otherModels;
  return model;
}
const number = (value: number): string => new Intl.NumberFormat(ui.locale).format(value);
const precision = (value: number): string =>
  new Intl.NumberFormat(ui.locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value);
const harnessName = (key: string): string => ui.data?.harnesses[key] ?? key;
const languageLabel = (language: MessageLanguage): string =>
  language === "other" ? copy().otherLabel : language === "zh" ? copy().zhLabel : copy().enLabel;

const currentFilters = () => ({
  harness: ($("harness") as HTMLSelectElement).value,
  model: ($("model") as HTMLSelectElement).value,
  language: ui.filtering,
});

function setInterface(): void {
  document.documentElement.lang = ui.locale === "zh" ? "zh-CN" : "en";
  document.title = copy().title + " · " + copy().brand;
  for (const node of document.querySelectorAll<HTMLElement>("[data-text]")) {
    const key = node.dataset.text as keyof Copy;
    const value = copy()[key];
    if (typeof value === "string") node.textContent = value;
  }
  for (const node of document.querySelectorAll<HTMLElement>("[data-label]")) {
    const value = copy()[node.dataset.label as keyof Copy];
    if (typeof value === "string") node.setAttribute("aria-label", value);
  }
  const toggle = $("interface-language");
  toggle.textContent = ui.locale === "zh" ? "EN" : "中文";
  toggle.setAttribute("aria-label", copy().otherUI);
  renderLanguageSegments();
  updateHarnesses();
  updateModels();
}

function renderLanguageSegments(): void {
  const labels: Record<string, string> = {
    all: copy().allLanguages,
    zh: copy().zhLabel,
    en: copy().enLabel,
    other: copy().otherLabel,
  };
  for (const button of document.querySelectorAll<HTMLButtonElement>("[data-language]")) {
    const value = button.dataset.language as keyof typeof labels;
    button.textContent = labels[value];
  }
}

function updateHarnesses(): void {
  const select = $("harness") as HTMLSelectElement;
  const selected = select.value;
  const ids = new Set([
    ...Object.keys(ui.data?.harnesses ?? {}),
    ...(ui.data?.entries ?? []).map(entry => entry.harness),
  ]);
  if (selected) ids.add(selected);
  select.replaceChildren(
    new Option(copy().allHarnesses, ""),
    ...[...ids].map(id => new Option(harnessName(id), id)),
  );
  select.value = selected;
}

function updateModels(): void {
  const select = $("model") as HTMLSelectElement;
  const selected = select.value;
  const models = new Set<string>((ui.data?.entries ?? []).map(entry => entry.model));
  if (selected.startsWith("model:")) models.add(selected.slice("model:".length));

  const groups = new Map<string, HTMLOptionElement[]>();
  for (const name of [...models].sort(compareModels)) {
    const family = familyOf(name);
    if (!groups.has(family)) groups.set(family, []);
    groups.get(family)!.push(new Option(modelLabel(name), "model:" + name));
  }

  const families = dom("optgroup");
  families.label = copy().modelFamilies;
  for (const id of [...new Set([...groups.keys(), OTHER_FAMILY])]) {
    if (id === OTHER_FAMILY && !groups.has(OTHER_FAMILY)) continue;
    families.append(new Option(copy().family(familyName(id)), "family:" + id));
  }

  const options: (HTMLOptionElement | HTMLOptGroupElement)[] = [new Option(copy().allModels, "")];
  if (families.childElementCount) options.push(families);
  for (const [id, children] of groups) {
    const group = dom("optgroup");
    group.label = familyName(id);
    group.append(...children);
    options.push(group);
  }
  select.replaceChildren(...options);
  select.value = selected;
  if (select.selectedIndex < 0) select.selectedIndex = 0;
}

/* ---------------------------------------------------------------- summary */

function renderSummary(rows: Combination[]): void {
  const total = totals(rows);
  const cards: { label: string; value: string }[] = [
    { label: copy().combinations, value: number(total.combinations) },
    { label: copy().messages, value: number(total.turns) },
    { label: copy().insults, value: number(total.hits) },
    { label: copy().overall, value: precision(total.score) },
  ];
  $("stats").replaceChildren(
    ...cards.map(card => {
      const figure = dom("div", "stat");
      figure.append(dom("span", "stat-label", card.label), dom("strong", "stat-value", card.value));
      return figure;
    }),
  );
}

/* ------------------------------------------------------------------ chart */

function renderAxis(maximum: number): void {
  const axis = $("chart-axis");
  const ticks = axisTicks(maximum, 4);
  axis.replaceChildren(
    ...ticks.map(tick => {
      const mark = dom("span", "axis-tick", precision(tick));
      mark.style.left = (100 * tick) / maximum + "%";
      return mark;
    }),
  );
}

function renderBars(rows: Combination[], ranks: Map<string, number>): void {
  const top = sortCombinations(rows, "score", -1, harnessName).slice(0, 6);
  const maximum = axisMaximum(Math.max(0, ...top.flatMap(row => Object.values(row.languages).map(entry => roastIndex(entry.hits, entry.turns)))));
  renderAxis(maximum);
  const languages: MessageLanguage[] = ui.filtering === "all"
    ? ["zh", "en", ...(rows.some(row => row.languages.other) ? ["other" as const] : [])] : [ui.filtering];

  const bar = (language: MessageLanguage, entry: Entry | undefined): HTMLElement => {
    const row = dom("span", "bar-line " + language);
    const track = dom("span", "track");
    track.setAttribute("aria-hidden", "true");
    if (entry) {
      const fill = dom("i", "fill");
      fill.style.width = (100 * roastIndex(entry.hits, entry.turns)) / maximum + "%";
      track.append(fill);
      row.title = `${languageLabel(language)} · ${number(entry.hits)} / ${number(entry.turns)}`;
    } else {
      row.classList.add("absent");
    }
    row.append(
      track,
      dom(
        "span",
        entry ? "bar-value" : "bar-value missing",
        entry ? precision(roastIndex(entry.hits, entry.turns)) : "—",
      ),
    );
    return row;
  };

  const elements = top.map(entry => {
    const row = dom("button", "bar-row");
    row.type = "button";
    const name = dom("span", "bar-name");
    const rank = ranks.get(entry.id);
    const title = dom("span", "bar-title");
    title.append(
      rank ? dom("span", "bar-rank", String(rank)) : dom("span", "bar-rank", "—"),
      dom("strong", "", modelLabel(entry.model)),
    );
    name.append(title, dom("small", "", harnessName(entry.harness)));
    const group = dom("span", "bar-group");
    group.append(...languages.map(language => bar(language, entry.languages[language])));
    row.append(name, group);
    row.addEventListener("click", () => {
      const target = document.querySelector<HTMLElement>(`[data-id="${CSS.escape(entry.id)}"]`);
      target?.scrollIntoView({ block: "center" });
      target?.focus({ preventScroll: true });
    });
    return row;
  });

  $("bars").replaceChildren(...elements);
  $("legend").hidden = !elements.length || languages.length === 1;
  $("chart-hint").textContent = (rows.length > 6 ? copy().topSix : "") + copy().units;
}

/* ------------------------------------------------------------------ table */

function renderTable(rows: Combination[], ranks: Map<string, number>): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>("[data-sort]")) {
    const header = button.closest("th");
    const active = button.dataset.sort === ui.key;
    if (active) header?.setAttribute("aria-sort", ui.order < 0 ? "descending" : "ascending");
    else header?.removeAttribute("aria-sort");
    const caret = button.querySelector("i");
    if (caret) caret.textContent = active ? (ui.order < 0 ? "↓" : "↑") : "↕";
  }

  const ordered = sortCombinations(rows, ui.key, ui.order, harnessName);
  $("rows").replaceChildren(
    ...ordered.map(entry => {
      const rank = ranks.get(entry.id) ?? 0;
      const row = dom("tr");
      row.tabIndex = -1;
      row.dataset.id = entry.id;
      const rankCell = dom("td", "rank", rank ? String(rank) : "—");
      const modelCell = dom("td", "", modelLabel(entry.model));
      if (familyOf(entry.model) === OTHER_FAMILY) modelCell.classList.add("subtle");
      row.append(
        rankCell,
        dom("td", "", harnessName(entry.harness)),
        modelCell,
        dom("td", "numeric", number(entry.turns)),
        dom("td", "numeric", number(entry.hits)),
        dom("td", "numeric strong", precision(entry.score)),
      );
      return row;
    }),
  );
}

/* ----------------------------------------------------------------- render */

function render(): void {
  const filters = currentFilters();
  const rows = combine(ui.data?.entries ?? [], filters);
  const ranks = rankByScore(rows);
  renderSummary(rows);
  renderBars(rows, ranks);
  renderTable(rows, ranks);

  const loaded = ui.state === "ready";
  const populated = loaded && rows.length > 0;
  $("skeleton").hidden = ui.state !== "loading";
  $("stats").hidden = !loaded;
  $("content").hidden = !populated;
  $("empty").hidden = populated || ui.state === "loading";

  $("retry").hidden = ui.state !== "error";
  const empty =
    ui.state === "error"
      ? [copy().failed, copy().failedDetails]
      : ui.data?.entries.length
        ? [copy().noMatch, copy().noMatchDetails]
        : [copy().noData, copy().noDataDetails];
  $("empty-title").textContent = empty[0];
  $("empty-details").textContent = empty[1];

  $("refresh").textContent = ui.data?.updatedAt
    ? copy().updated + new Date(ui.data.updatedAt).toLocaleString(ui.locale)
    : copy().noSnapshot;

  const all = combine(ui.data?.entries ?? [], { harness: "", model: "", language: "all" }).length;
  $("result-count").textContent =
    rows.length === all ? copy().count(number(rows.length)) : copy().ofTotal(number(rows.length), number(all));
}

/* ------------------------------------------------------------------- data */

async function load({ quiet = false } = {}): Promise<void> {
  activeRequest?.abort();
  const request = new AbortController();
  activeRequest = request;
  if (!quiet) {
    ui.state = "loading";
    ui.data = null;
    render();
  }
  try {
    const response = await fetch("/api/v1/leaderboard", { signal: request.signal, cache: "no-store" });
    if (!response.ok) throw new Error("snapshot unavailable");
    const data = await response.json() as Snapshot;
    if (request.signal.aborted) return;
    ui.data = data;
    ui.state = "ready";
    updateHarnesses();
    updateModels();
    render();
  } catch {
    if (request.signal.aborted || (quiet && ui.state === "ready")) return;
    ui.state = "error";
    render();
  }
}

/* ----------------------------------------------------------------- events */

$("interface-language").addEventListener("click", () => {
  ui.locale = ui.locale === "zh" ? "en" : "zh";
  try {
    localStorage.setItem(languagePreference, ui.locale);
  } catch {
    // Browser storage is optional.
  }
  setInterface();
  render();
});

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-language]")) {
  button.addEventListener("click", () => {
    ui.filtering = button.dataset.language as MessageLanguage | "all";
    for (const other of document.querySelectorAll<HTMLButtonElement>("[data-language]")) {
      other.setAttribute("aria-pressed", String(other === button));
    }
    render();
  });
}

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-sort]")) {
  button.addEventListener("click", () => {
    const key = button.dataset.sort as SortKey;
    ui.order = ui.key === key ? ((ui.order === 1 ? -1 : 1) as 1 | -1) : (key === "model" || key === "harness" ? 1 : -1);
    ui.key = key;
    render();
  });
}

$("retry").addEventListener("click", () => void load());
$("harness").addEventListener("change", render);
$("model").addEventListener("change", render);

setInterface();
void load();
setInterval(() => {
  if (!document.hidden) void load({ quiet: true });
}, 60_000);
