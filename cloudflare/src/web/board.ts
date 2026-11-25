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
  sortCombinations,
  totals,
  type Combination,
  type Language,
  type MessageLanguage,
  type Snapshot,
  type SortKey,
  type GroupBy,
  type Filters,
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
  download: string;
  modelBoard: string;
  combinationBoard: string;
  byHarnessBoard: string;
  byModelBoard: string;
  models: string;
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
    download: "下载 macOS 版",
    modelBoard: "模型榜",
    combinationBoard: "模型 × Harness 组合榜",
    byHarnessBoard: "固定 Harness，看模型",
    byModelBoard: "固定模型，看 Harness",
    models: "模型",
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
    noMatch: "没有匹配的数据",
    noMatchDetails: "试试切换语言、模型或运行工具。",
    failed: "暂时读不到榜单",
    failedDetails: "请稍后重试。",
    topSix: "前 6 名 · ",
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
    download: "Download for macOS",
    modelBoard: "Model rankings",
    combinationBoard: "Model × harness combinations",
    byHarnessBoard: "Compare models in one harness",
    byModelBoard: "Compare harnesses for one model",
    models: "Models",
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
    noMatch: "No matching data",
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
interface Board {
  id: string;
  groupBy: GroupBy;
  language: MessageLanguage | "all";
  key: SortKey;
  order: 1 | -1;
}

const boards: Board[] = [
  { id: "models", groupBy: "model", language: "all", key: "score", order: -1 },
  { id: "combinations", groupBy: "combination", language: "all", key: "score", order: -1 },
  { id: "by-harness", groupBy: "model", language: "all", key: "score", order: -1 },
  { id: "by-model", groupBy: "harness", language: "all", key: "score", order: -1 },
];

const currentFilters = (board: Board): Filters => ({
  harness: $<HTMLSelectElement>(board.id + "-harness")?.value ?? "",
  model: $<HTMLSelectElement>(board.id + "-model")?.value ?? "",
  language: board.language,
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
  updateOptions();
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

function updateOptions(): void {
  const entries = ui.data?.entries ?? [];
  const harnesses = [...new Set(entries.map(entry => entry.harness))]
    .sort((a, b) => harnessName(a).localeCompare(harnessName(b)));
  const models = [...new Set(entries.map(entry => entry.model))].sort(compareModels);
  for (const board of boards) {
    const harness = $<HTMLSelectElement>(board.id + "-harness");
    if (harness) {
      const selected = harness.value;
      const ids = [...harnesses];
      if (selected && !ids.includes(selected)) ids.push(selected);
      harness.replaceChildren(
        ...(board.id === "combinations" ? [new Option(copy().allHarnesses, "")] : []),
        ...ids.map(id => new Option(harnessName(id), id)),
      );
      if (selected) harness.value = selected;
      harness.disabled = !ids.length;
    }
    const model = $<HTMLSelectElement>(board.id + "-model");
    if (model) {
      const selected = model.value;
      const names = [...models];
      if (selected.startsWith("model:") && !names.includes(selected.slice(6))) names.push(selected.slice(6));
      const groups = new Map<string, HTMLOptionElement[]>();
      for (const name of names.sort(compareModels)) {
        const family = familyOf(name);
        if (!groups.has(family)) groups.set(family, []);
        groups.get(family)!.push(new Option(modelLabel(name), "model:" + name));
      }
      const options: (HTMLOptionElement | HTMLOptGroupElement)[] = [];
      if (board.id === "combinations") {
        options.push(new Option(copy().allModels, ""));
        const families = dom("optgroup");
        families.label = copy().modelFamilies;
        for (const id of groups.keys()) families.append(new Option(copy().family(familyName(id)), "family:" + id));
        if (selected.startsWith("family:") && !groups.has(selected.slice(7))) {
          families.append(new Option(copy().family(familyName(selected.slice(7))), selected));
        }
        if (families.childElementCount) options.push(families);
      }
      for (const [id, children] of groups) {
        const group = dom("optgroup");
        group.label = familyName(id);
        group.append(...children);
        options.push(group);
      }
      model.replaceChildren(...options);
      if (selected) model.value = selected;
      model.disabled = !names.length;
    }
  }
}

/* ---------------------------------------------------------------- summary */

function renderSummary(rows: Combination[]): void {
  const total = totals(rows);
  const cards: { label: string; value: string }[] = [
    { label: copy().models, value: number(total.combinations) },
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

function renderBars(board: Board, rows: Combination[], ranks: Map<string, number>): void {
  const top = sortCombinations(rows, "score", -1, harnessName).slice(0, 6);
  const maximum = axisMaximum(Math.max(0, ...top.map(row => row.score)));
  $(board.id + "-axis").replaceChildren(...axisTicks(maximum).map(tick => {
    const mark = dom("span", "axis-tick", precision(tick));
    mark.style.left = (100 * tick) / maximum + "%";
    return mark;
  }));
  $(board.id + "-bars").replaceChildren(...top.map(entry => {
    const row = dom("button", "bar-row");
    row.type = "button";
    const name = dom("span", "bar-name");
    const title = dom("span", "bar-title");
    const label = board.groupBy === "harness" ? harnessName(entry.harness) : modelLabel(entry.model);
    title.append(dom("span", "bar-rank", String(ranks.get(entry.id))), dom("strong", "", label));
    name.append(title);
    if (board.groupBy === "combination") name.append(dom("small", "", harnessName(entry.harness)));
    const line = dom("span", "bar-line zh");
    const track = dom("span", "track");
    track.setAttribute("aria-hidden", "true");
    const fill = dom("i", "fill");
    fill.style.width = (100 * entry.score) / maximum + "%";
    track.append(fill);
    line.append(track, dom("span", "bar-value", precision(entry.score)));
    row.title = `${label} · ${copy().hits}: ${number(entry.hits)} / ${copy().turns}: ${number(entry.turns)}`;
    row.append(name, line);
    row.addEventListener("click", () => {
      const target = $(board.id + "-rows").querySelector<HTMLElement>(`[data-id="${CSS.escape(entry.id)}"]`);
      target?.scrollIntoView({ block: "center" });
      target?.focus({ preventScroll: true });
    });
    return row;
  }));
  $(board.id + "-hint").textContent = (rows.length > 6 ? copy().topSix : "") + copy().units;
}

function renderTable(board: Board, rows: Combination[], ranks: Map<string, number>): void {
  for (const button of $(board.id).querySelectorAll<HTMLButtonElement>("[data-sort]")) {
    const header = button.closest("th");
    const active = button.dataset.sort === board.key;
    if (active) header?.setAttribute("aria-sort", board.order < 0 ? "descending" : "ascending");
    else header?.removeAttribute("aria-sort");
    const caret = button.querySelector("i");
    if (caret) caret.textContent = active ? (board.order < 0 ? "↓" : "↑") : "↕";
  }
  $(board.id + "-rows").replaceChildren(...sortCombinations(rows, board.key, board.order, harnessName).map(entry => {
    const row = dom("tr");
    row.tabIndex = -1;
    row.dataset.id = entry.id;
    row.append(dom("td", "rank", String(ranks.get(entry.id))));
    if (board.groupBy !== "harness") {
      row.append(dom("td", familyOf(entry.model) === OTHER_FAMILY ? "subtle" : "", modelLabel(entry.model)));
    }
    if (board.groupBy !== "model") row.append(dom("td", "", harnessName(entry.harness)));
    row.append(
      dom("td", "numeric", number(entry.turns)),
      dom("td", "numeric", number(entry.hits)),
      dom("td", "numeric strong", precision(entry.score)),
    );
    return row;
  }));
}

function render(): void {
  const entries = ui.data?.entries ?? [];
  for (const board of boards) {
    const filters = currentFilters(board);
    // Fixed-dimension boards always require a concrete selection.
    const hasSelection = (board.id !== "by-harness" || !!filters.harness)
      && (board.id !== "by-model" || filters.model.startsWith("model:"));
    const rows = hasSelection ? combine(entries, filters, board.groupBy) : [];
    const ranks = rankByScore(rows);
    if (board.id === "models") renderSummary(rows);
    renderBars(board, rows, ranks);
    renderTable(board, rows, ranks);
    $(board.id + "-results").hidden = !rows.length;
    $(board.id + "-empty").hidden = !!rows.length;
    const label = board.groupBy === "model" ? copy().models : board.groupBy === "harness" ? copy().harness : copy().combinations;
    $(board.id + "-count").textContent = `${number(rows.length)} ${label}`;
  }
  const populated = ui.state === "ready" && entries.length > 0;
  $("skeleton").hidden = ui.state !== "loading";
  $("content").hidden = !populated;
  $("empty").hidden = populated || ui.state === "loading";
  $("retry").hidden = ui.state !== "error";
  $("empty-title").textContent = ui.state === "error" ? copy().failed : copy().noData;
  $("empty-details").textContent = ui.state === "error" ? copy().failedDetails : copy().noDataDetails;
  $("refresh").hidden = ui.state !== "ready";
  $("refresh").textContent = ui.data?.updatedAt
    ? copy().updated + new Date(ui.data.updatedAt).toLocaleString(ui.locale)
    : copy().noSnapshot;
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
    updateOptions();
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

for (const board of boards) {
  const section = $(board.id);
  for (const button of section.querySelectorAll<HTMLButtonElement>("[data-language]")) {
    button.addEventListener("click", () => {
      board.language = button.dataset.language as MessageLanguage | "all";
      for (const other of section.querySelectorAll<HTMLButtonElement>("[data-language]")) {
        other.setAttribute("aria-pressed", String(other === button));
      }
      render();
    });
  }
  for (const button of section.querySelectorAll<HTMLButtonElement>("[data-sort]")) {
    button.addEventListener("click", () => {
      const key = button.dataset.sort as SortKey;
      board.order = board.key === key ? (board.order === 1 ? -1 : 1) : (key === "model" || key === "harness" ? 1 : -1);
      board.key = key;
      render();
    });
  }
  for (const select of section.querySelectorAll("select")) select.addEventListener("change", render);
}
$("retry").addEventListener("click", () => void load());

setInterface();
void load();
setInterval(() => {
  if (!document.hidden) void load({ quiet: true });
}, 60_000);
