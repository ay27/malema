const translations = {
  zh: {
    brand: "骂了吗", title: "全球被骂榜", period: "统计期次", current: "本期", previous: "上期",
    messageLanguage: "消息语言", allLanguages: "全部", harness: "运行工具", allHarnesses: "全部工具",
    model: "回复模型", allModels: "全部模型", modelFamilies: "模型系列", otherModels: "其他模型",
    family: name => name + " 系列", compare: "组合对比", details: "组合明细", turns: "真人消息",
    hits: "明确挨骂", score: "被骂指数",
    top: "回到顶部 ↑", retry: "再试一次", loading: "正在读取榜单", noData: "样本还在积累",
    noMatch: "没有匹配的组合", noMatchDetails: "试试切换语言、模型或运行工具。",
    failed: "暂时读不到榜单", failedDetails: "请稍后重试。", units: "每千条消息的明确挨骂数",
    topSix: "前 6 组 · ", updated: "快照更新于 ", noSnapshot: "等待首份榜单快照",
    thresholds: (turns, contributors) => `同一工具、模型和语言至少有 ${turns} 条消息，来自 ${contributors} 个贡献安装。`,
    count: n => `${n} 个组合`, otherUI: "Switch interface to English", zhLabel: "中文", enLabel: "English"
  },
  en: {
    brand: "Malema", title: "Global AI Roasting Board", period: "Reporting period", current: "Current", previous: "Previous",
    messageLanguage: "Message language", allLanguages: "All", harness: "Harness", allHarnesses: "All harnesses",
    model: "Reply model", allModels: "All models", modelFamilies: "Model families", otherModels: "Other models",
    family: name => name + " family", compare: "Top combinations", details: "All combinations", turns: "Human messages",
    hits: "Clear insults", score: "Roast index",
    top: "Back to top ↑", retry: "Try again", loading: "Loading rankings", noData: "Still collecting samples",
    noMatch: "No matching combinations", noMatchDetails: "Try another language, model, or harness.",
    failed: "Could not load rankings", failedDetails: "Please try again shortly.", units: "Clear insults per 1,000 human messages",
    topSix: "Top 6 · ", updated: "Snapshot updated ", noSnapshot: "Waiting for the first snapshot",
    thresholds: (turns, contributors) => `Each harness, model and language needs at least ${turns} messages from ${contributors} contributing installations.`,
    count: n => `${n} combinations`, otherUI: "切换至中文界面", zhLabel: "中文", enLabel: "English"
  }
};

const $ = id => document.getElementById(id);
const dom = (tag, name = "", value) => {
  const element = document.createElement(tag);
  if (name) element.className = name;
  if (value !== undefined) element.textContent = value;
  return element;
};
const ui = {
  locale: document.documentElement.lang.startsWith("en") ? "en" : "zh",
  filtering: "all", key: "score", order: -1, state: "loading", data: null
};
let activeRequest;
const copy = () => translations[ui.locale];
const number = n => new Intl.NumberFormat(ui.locale).format(n);
const precision = n => new Intl.NumberFormat(ui.locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(n);
const roastIndex = entry => 1000 * entry.hits / entry.turns;
const harnessName = key => ui.data?.harnesses[key] ?? key;
const label = language => language === "zh" ? copy().zhLabel : copy().enLabel;
const modelFamilies = [
  { id:"gpt", name:"GPT", prefix:/^(?:openai[/:])?(?:gpt|o\d+)(?:[-_.]|$)/i },
  { id:"anthropic", name:"Anthropic", prefix:/^(?:claude|anthropic)(?:[-_. /:]|$)/i },
  { id:"hunyuan", name:"Hunyuan", prefix:/^(?:(?:tencent[/:])?hunyuan|混元)(?:[-_. /:]|$)/i },
  { id:"glm", name:"GLM", prefix:/^(?:(?:zhipu|zai|z\.ai)[/:])?(?:glm|chatglm)(?:[-_. /:]|\d|$)/i },
  { id:"deepseek", name:"DeepSeek", prefix:/^deepseek(?:[-_. /:]|$)/i },
  { id:"gemini", name:"Gemini", prefix:/^(?:google[/:])?gemini(?:[-_. /:]|$)/i },
  { id:"qwen", name:"Qwen", prefix:/^(?:(?:alibaba[/:])?qwen|通义)(?:[-_. /:]|\d|$)/i }
];
const familyOf = model => modelFamilies.find(family => family.prefix.test(model))?.id ?? "other";
const familyOrder = new Map([...modelFamilies.map(family => family.id), "other"].map((id, index) => [id, index]));
const modelNames = new Intl.Collator("en", { numeric: true, sensitivity: "base" });
const compareModels = (a, b) => familyOrder.get(familyOf(a)) - familyOrder.get(familyOf(b)) || modelNames.compare(a, b);

function setInterface() {
  document.documentElement.lang = ui.locale === "zh" ? "zh-CN" : "en";
  document.title = copy().title + " · " + copy().brand;
  document.querySelectorAll("[data-text]").forEach(node => node.textContent = copy()[node.dataset.text]);
  updateModels();
  $("interface-language").textContent = ui.locale === "zh" ? "EN" : "中文";
  $("interface-language").setAttribute("aria-label", copy().otherUI);
}

function updateHarnesses(data) {
  const previous = $("harness").value;
  const all = new Option(copy().allHarnesses, "");
  all.dataset.text = "allHarnesses";
  const options = Object.entries(data.harnesses).map(([id, title]) => new Option(title, id));
  $("harness").replaceChildren(all, ...options);
  if (options.some(option => option.value === previous)) $("harness").value = previous;
}

function updateModels() {
  const select = $("model");
  const selectedModel = select.value;
  const models = new Set((ui.data?.entries ?? []).map(entry => entry.model));
  if (selectedModel.startsWith("model:")) models.add(selectedModel.slice("model:".length));
  const groups = new Map();
  for (const name of [...models].sort(compareModels)) {
    const family = familyOf(name);
    if (!groups.has(family)) groups.set(family, []);
    groups.get(family).push(new Option(name, "model:" + name));
  }
  const families = dom("optgroup");
  families.label = copy().modelFamilies;
  for (const family of modelFamilies) {
    families.append(new Option(copy().family(family.name), "family:" + family.id));
  }
  if (groups.has("other")) families.append(new Option(copy().otherModels, "family:other"));
  const options = [new Option(copy().allModels, ""), families];
  for (const [id, children] of groups) {
    const group = dom("optgroup");
    group.label = modelFamilies.find(family => family.id === id)?.name ?? copy().otherModels;
    group.append(...children);
    options.push(group);
  }
  select.replaceChildren(...options);
  select.value = selectedModel;
  if (select.selectedIndex < 0) select.selectedIndex = 0;
}

function combine() {
  const grouped = new Map();
  const model = $("model").value;
  const harness = $("harness").value;
  for (const entry of ui.data?.entries ?? []) {
    if ((harness && entry.harness !== harness)
        || (model && model !== "model:" + entry.model && model !== "family:" + familyOf(entry.model))
        || (ui.filtering !== "all" && ui.filtering !== entry.language)) continue;
    const id = JSON.stringify([entry.harness, entry.model]);
    if (!grouped.has(id)) grouped.set(id, {
      id, harness: entry.harness, model: entry.model, hits: 0, turns: 0, languages: {}
    });
    const group = grouped.get(id);
    group.hits += entry.hits;
    group.turns += entry.turns;
    group.languages[entry.language] = entry;
  }
  return Array.from(grouped.values(), group => ({ ...group, score: roastIndex(group) }));
}

function sort(entries) {
  const { key, order } = ui;
  const value = (a, b) => {
    if (key === "harness") return harnessName(a.harness).localeCompare(harnessName(b.harness));
    if (key === "model") return compareModels(a.model, b.model);
    return a[key] - b[key];
  };
  return entries.toSorted((a, b) => order * value(a, b) || b.turns - a.turns || a.id.localeCompare(b.id));
}

function renderTable(entries) {
  for (const button of document.querySelectorAll("[data-sort]")) {
    const header = button.closest("th");
    const active = button.dataset.sort === ui.key;
    if (active) header.setAttribute("aria-sort", ui.order < 0 ? "descending" : "ascending");
    else header.removeAttribute("aria-sort");
    button.lastElementChild.textContent = active ? ui.order < 0 ? "↓" : "↑" : "↕";
  }
  const elements = new Map();
  for (const entry of sort(entries)) {
    const row = dom("tr");
    row.tabIndex = -1;
    row.append(
      dom("td", "", harnessName(entry.harness)), dom("td", "", entry.model),
      dom("td", "", number(entry.turns)), dom("td", "", number(entry.hits)), dom("td", "", precision(entry.score))
    );
    elements.set(entry.id, row);
  }
  $("rows").replaceChildren(...elements.values());
  $("result-count").textContent = copy().count(number(entries.length));
  return elements;
}

function renderBars(entries, tableRows) {
  const top = entries.toSorted((a, b) => b.score - a.score || b.turns - a.turns).slice(0, 6);
  const maximum = Math.max(1, ...top.flatMap(row => Object.values(row.languages).map(roastIndex)));
  const langs = ui.filtering === "all" ? ["zh", "en"] : [ui.filtering];
  const bar = (language, entry) => {
    const row = dom("span", "bar-line " + language);
    const track = dom("span", "track");
    track.setAttribute("aria-hidden", "true");
    if (entry) {
      const fill = dom("i", "fill");
      fill.style.width = (100 * roastIndex(entry) / maximum) + "%";
      track.append(fill);
      row.title = `${label(language)} · ${number(entry.hits)} / ${number(entry.turns)}`;
    }
    row.append(track, dom("span", entry ? "bar-value" : "bar-value missing", entry ? precision(roastIndex(entry)) : "—"));
    return row;
  };
  const rows = top.map(entry => {
    const button = dom("button", "bar-row");
    button.type = "button";
    const name = dom("span", "bar-name");
    name.append(dom("strong", "", entry.model), dom("small", "", harnessName(entry.harness)));
    const group = dom("span", "bar-group");
    group.append(...langs.map(language => bar(language, entry.languages[language])));
    button.append(name, group);
    button.addEventListener("click", () => {
      const row = tableRows.get(entry.id);
      row.scrollIntoView({ block: "center" });
      row.focus({ preventScroll: true });
    });
    return button;
  });
  $("bars").replaceChildren(...rows);
  $("legend").hidden = !rows.length || langs.length === 1;
  $("chart-hint").textContent = (entries.length > 6 ? copy().topSix : "") + copy().units;
}

function render() {
  const entries = combine();
  renderBars(entries, renderTable(entries));
  $("empty").hidden = entries.length > 0;
  $("retry").hidden = ui.state !== "error";
  const threshold = ui.data?.thresholds ?? { turns: 300, contributors: 5 };
  const limits = copy().thresholds(number(threshold.turns), number(threshold.contributors));
  const empty = {
    loading: [copy().loading, ""], error: [copy().failed, copy().failedDetails],
    ready: ui.data?.entries.length ? [copy().noMatch, copy().noMatchDetails] : [copy().noData, limits]
  }[ui.state];
  $("empty-title").textContent = empty[0];
  $("empty-details").textContent = empty[1];
  $("period-range").textContent = ui.data ? `${ui.data.start} – ${ui.data.end} UTC` : "";
  $("refresh").textContent = ui.data?.updatedAt
    ? copy().updated + new Date(ui.data.updatedAt).toLocaleString(ui.locale) : copy().noSnapshot;
}

async function load({ quiet = false } = {}) {
  activeRequest?.abort();
  const request = new AbortController();
  activeRequest = request;
  if (!quiet) {
    ui.state = "loading";
    ui.data = null;
    render();
  }
  try {
    const response = await fetch("/api/v1/leaderboard?period=" + $("period").value, { signal: request.signal });
    if (!response.ok) throw new Error("snapshot unavailable");
    const data = await response.json();
    if (request.signal.aborted) return;
    ui.data = data;
    ui.state = "ready";
    updateHarnesses(data);
    updateModels();
    render();
  } catch {
    if (request.signal.aborted || (quiet && ui.state === "ready")) return;
    ui.state = "error";
    render();
  }
}

$("interface-language").addEventListener("click", () => {
  ui.locale = ui.locale === "zh" ? "en" : "zh";
  setInterface();
  render();
});
for (const button of document.querySelectorAll("[data-language]")) {
  button.addEventListener("click", () => {
    ui.filtering = button.dataset.language;
    for (const option of document.querySelectorAll("[data-language]")) {
      option.setAttribute("aria-pressed", String(option === button));
    }
    render();
  });
}
for (const button of document.querySelectorAll("[data-sort]")) {
  button.addEventListener("click", () => {
    const key = button.dataset.sort;
    ui.order = ui.key === key ? -ui.order : (key === "model" || key === "harness" ? 1 : -1);
    ui.key = key;
    render();
  });
}
$("period").addEventListener("change", () => load());
$("retry").addEventListener("click", () => load());
$("harness").addEventListener("change", render);
$("model").addEventListener("change", render);
setInterface();
load();
setInterval(() => { if (!document.hidden) load({ quiet: true }); }, 60_000);
