use crate::detect::{self, Hit};
use crate::i18n;
use crate::review_state;
use crate::sources::{self, InferredDay, Source, SourceStats};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

#[derive(Serialize)]
pub struct Entry {
    source: Source,
    date: Option<String>,
    date_basis: Option<&'static str>,
    model: Option<String>,
    harness_version: Option<String>,
    session: String,
    line: usize,
    excerpt: String,
    hit: Hit,
}

#[derive(Default, Serialize)]
struct Count {
    turns: usize,
    hits: usize,
}

#[derive(Serialize)]
struct PeriodCount {
    client: &'static str,
    date: Option<String>,
    model: Option<String>,
    harness_version: Option<String>,
    #[serde(flatten)]
    count: Count,
}

#[derive(Serialize)]
pub struct Report {
    ruleset: &'static str,
    sources: Vec<SourceStats>,
    hits: Vec<Entry>,
    rows: Vec<PeriodCount>,
}

pub fn build(home: &Path, use_env: bool) -> Report {
    build_observed(home, use_env, |_, _, _| {})
}

pub fn build_observed(
    home: &Path,
    use_env: bool,
    mut observe: impl FnMut(Source, &sources::Turn, bool),
) -> Report {
    let verdicts = review_state::load(home).unwrap_or_else(|error| {
        eprintln!("could not load review verdicts: {error}");
        review_state::Verdicts::new()
    });
    let mut hits = Vec::new();
    let mut totals = BTreeMap::<_, Count>::new();
    let sources = sources::scan(home, use_env, |stats, turn| {
        let rule_hit = detect::detect(&turn.text);
        observe(stats.source, &turn, rule_hit.is_some());
        let hit = if verdicts.is_empty() {
            rule_hit
        } else {
            let key = review_state::message_key(stats.source, &turn);
            match verdicts.get(key.as_slice()) {
                Some(verdict) => verdict.hit(rule_hit, &turn.text),
                None => rule_hit,
            }
        };
        let bucket = totals
            .entry((
                stats.source.label(),
                turn.date.clone(),
                turn.model.clone(),
                turn.harness_version.clone(),
            ))
            .or_default();
        bucket.turns += 1;
        bucket.hits += usize::from(hit.is_some());
        if let Some(hit) = hit {
            stats.hits += 1;
            hits.push(Entry {
                source: stats.source,
                date: turn.date,
                date_basis: match turn.inferred_day {
                    Some(InferredDay::SameDayNeighbors) => Some("neighbors"),
                    Some(InferredDay::FileMetadata) => Some("file"),
                    None => None,
                },
                model: turn.model,
                harness_version: turn.harness_version,
                session: turn.session,
                line: turn.line,
                excerpt: excerpt(&turn.text, &hit.evidence),
                hit,
            });
        }
    });
    hits.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.session.cmp(&b.session))
            .then(a.line.cmp(&b.line))
    });
    let rows = totals
        .into_iter()
        .map(
            |((client, date, model, harness_version), count)| PeriodCount {
                client,
                date,
                model,
                harness_version,
                count,
            },
        )
        .collect();
    Report {
        ruleset: detect::RULESET,
        sources,
        hits,
        rows,
    }
}

fn review_dialog(provider: &str, scope: &str) -> String {
    include_str!("review.html")
        .replace("<!-- REVIEW_TITLE -->", t("AI 复核", "AI review"))
        .replace("<!-- REVIEW_CLOSE -->", t("关闭", "close"))
        .replace("<!-- REVIEW_PROVIDER_LABEL -->", t("判定工具", "reviewer"))
        .replace("<!-- REVIEW_SCOPE_LABEL -->", t("复核范围", "review scope"))
        .replace("<!-- REVIEW_PROVIDER -->", provider)
        .replace("<!-- REVIEW_SCOPE -->", scope)
        .replace(
            "<!-- REVIEW_KEY_PLACEHOLDER -->",
            t(
                "填写密钥，或使用 TYPESAFE_API_KEY",
                "enter a key, or set TYPESAFE_API_KEY",
            ),
        )
        .replace("<!-- REVIEW_ENDPOINT_LABEL -->", t("接口地址", "endpoint"))
        .replace("<!-- REVIEW_MODEL_LABEL -->", t("模型名", "model"))
        .replace(
            "<!-- REVIEW_MODEL_PLACEHOLDER -->",
            t("填写接口的模型名", "the model name for this endpoint"),
        )
        .replace(
            "<!-- REVIEW_CUSTOM_KEY_LABEL -->",
            t("API Key（可选）", "API key (optional)"),
        )
        .replace(
            "<!-- REVIEW_CUSTOM_KEY_PLACEHOLDER -->",
            t(
                "接口无需鉴权时留空",
                "leave empty if the endpoint needs no key",
            ),
        )
        .replace(
            "<!-- REVIEW_NOTE -->",
            t(
                "开始后，脱敏片段会交给所选 Agent 或模型服务。",
                "Once started, redacted excerpts go to the chosen agent or model service.",
            ),
        )
        .replace("<!-- REVIEW_STOP -->", t("停止", "stop"))
        .replace("<!-- REVIEW_START -->", t("开始复核", "start review"))
}

fn t<'a>(zh: &'a str, en: &'a str) -> &'a str {
    i18n::pick(zh, en)
}

fn excerpt(text: &str, evidence: &str) -> String {
    let target = evidence.to_lowercase();
    let chosen = text
        .lines()
        .find(|line| {
            line.nfkc()
                .collect::<String>()
                .to_lowercase()
                .contains(&target)
        })
        .unwrap_or_else(|| text.lines().next().unwrap_or_default());
    let chars: Vec<char> = chosen.chars().collect();
    if chars.len() <= 440 {
        return chosen.to_owned();
    }
    let normalized = chosen.nfkc().collect::<String>().to_lowercase();
    let offset = normalized
        .find(&target)
        .map_or(0, |index| normalized[..index].chars().count());
    let start = offset.saturating_sub(150).min(chars.len() - 440);
    format!(
        "{}{}{}",
        if start == 0 { "" } else { "…" },
        chars[start..start + 440].iter().collect::<String>(),
        if start + 440 == chars.len() {
            ""
        } else {
            "…"
        }
    )
}

fn escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

impl Report {
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "ruleset": self.ruleset,
            "turns": self.sources.iter().map(|s| s.turns).sum::<usize>(),
            "hits": self.hits.len(),
            "sources": self.sources,
        })
    }

    pub fn data(&self) -> serde_json::Value {
        let clients: Vec<_> = self
            .sources
            .iter()
            .map(|stats| {
                let mut row = serde_json::to_value(stats).expect("serializable source");
                row["client"] = stats.source.label().into();
                row
            })
            .collect();
        serde_json::json!({"rows":self.rows, "clients":clients})
    }

    pub fn records_html(&self) -> String {
        let mut html = String::new();
        for entry in &self.hits {
            let source = entry.source.label();
            let source_label = entry.harness_version.as_deref().map_or_else(
                || source.to_owned(),
                |version| format!("{source} {}", escape(version)),
            );
            let date = entry.date.as_deref().unwrap_or("");
            let visible_date = match (entry.date.as_deref(), entry.date_basis) {
                (Some(date), Some(basis)) => {
                    let note = date_basis_note(basis);
                    if i18n::lang() == i18n::Lang::Zh {
                        format!("{date}（{note}）")
                    } else {
                        format!("{date} ({note})")
                    }
                }
                (Some(date), None) => date.to_owned(),
                _ => t("日期未知", "unknown date").to_owned(),
            };
            let model = escape(
                entry
                    .model
                    .as_deref()
                    .unwrap_or(t("模型未知", "unknown model")),
            );
            let session = escape(&entry.session);
            let evidence = escape(&entry.hit.evidence);
            let excerpt = escape(&entry.excerpt);
            let code = entry.hit.rule;
            let reason = escape(&entry.hit.reason);
            let label = if code.starts_with("ai.") {
                t("AI 复核结论", "AI review verdict")
            } else {
                t("为什么计入", "why it counts")
            };
            let position = if entry.line == 0 {
                t("Cursor 会话数据库", "Cursor session database").to_owned()
            } else if i18n::lang() == i18n::Lang::Zh {
                format!("第 {} 行", entry.line)
            } else {
                format!("line {}", entry.line)
            };
            let _ = write!(
                html,
                "<details class='record' data-client='{source}' data-date='{date}'><summary><span><strong>{evidence}</strong><small>{source_label} · {model} · {visible_date}</small></span></summary><div class='record-body'><p><b>{label}</b>　{reason}</p><blockquote>{excerpt}</blockquote><small>{session} · {position} · {code}</small></div></details>"
            );
        }
        html
    }

    pub fn render(&self, desktop: bool) -> String {
        let mut body = format!(
            r#"<div class="shell"><aside><div class="brand"><img src="data:image/png;base64,{}" width="32" height="32" alt="">{}</div><nav aria-label="{}">
          <button data-tab="overview" aria-current="page"><span class="nav-icon" aria-hidden="true">◫</span> {}</button>
          <button data-tab="records"><span class="nav-icon" aria-hidden="true">≡</span> {}</button>
          <button data-tab="index"><span class="nav-icon" aria-hidden="true">▥</span> {}</button>
          <button data-tab="sources"><span class="nav-icon" aria-hidden="true">▤</span> {}</button>
          "#,
            include_str!("../assets/brand-mark.base64").trim(),
            t("骂了吗", "Malema"),
            t("报表导航", "Report navigation"),
            t("总览", "Overview"),
            t("挨骂记录", "Insults"),
            t("被骂指数", "Insult index"),
            t("数据来源", "Sources"),
        );
        if desktop {
            let _ = write!(
                body,
                r#"<button id="open-review-nav" aria-haspopup="dialog"><span class="nav-icon" aria-hidden="true">✓</span> {}</button>"#,
                t("AI 复核", "AI review")
            );
            let _ = write!(
                body,
                r#"<button id="open-leaderboard" type="button" title="{}"><span class="nav-icon" aria-hidden="true">↗</span> {}</button>"#,
                t("在默认浏览器打开", "open in the default browser"),
                t("全球榜单", "Leaderboard")
            );
        }
        body.push_str("</nav>");
        if desktop {
            let _ = write!(
                body,
                r#"<div class="contribution-control">
              <button class="contribution-switch" id="contribution-switch" type="button" role="switch" aria-checked="false" aria-label="{}" title="{}" disabled>
                <span class="contribution-label">{}<small>{}</small></span><span class="switch-track" aria-hidden="true"></span>
              </button>
              <div class="contribution-status" id="contribution-status" role="status" hidden>
                <p id="contribution-summary" hidden></p>
                <p id="contribution-time" hidden></p>
              </div>
              <p class="contribution-error" id="contribution-error" role="status" hidden></p>
            </div>"#,
                t(
                    "匿名提交，参与全球排行",
                    "submit anonymously to the global leaderboard"
                ),
                t(
                    "上报规则扫描的匿名计数。手动开启时包含最近 7 天。",
                    "Shares anonymous counts from the rule-based scan. Turning it on manually includes the last 7 days."
                ),
                t("匿名提交", "Anonymous"),
                t("参与全球排行", "join the leaderboard"),
            );
            let _ = write!(
                body,
                r#"<button class="language-toggle" id="language-toggle" type="button" title="{}">{}</button>
              <button class="repository-link" id="open-repository" type="button" title="{}">{}</button>"#,
                t("切换界面语言", "switch the interface language"),
                t("EN", "中文"),
                t(
                    "在浏览器打开仓库，反馈问题或点 Star",
                    "open the repository to file an issue or star it"
                ),
                t("GitHub · 反馈与 Star", "GitHub · issues and stars")
            );
        } else {
            let _ = write!(
                body,
                r#"<div class="local">{}</div>"#,
                t("本机离线", "local only")
            );
        }
        let _ = write!(
            body,
            r#"</aside><main>
          <div class="toolbar"><strong id="current-title">{}</strong><div class="controls"><div id="toolbar-filters">
          "#,
            t("总览", "Overview")
        );
        let sources: Vec<_> = Source::ALL
            .iter()
            .map(|source| (source.label(), source.label()))
            .collect();
        let mut source_choices = vec![("", t("全部来源", "all sources"))];
        source_choices.extend(sources);
        filter_markup(
            &mut body,
            "source",
            t("筛选来源", "filter by source"),
            &source_choices,
        );
        filter_markup(
            &mut body,
            "range",
            t("筛选时间", "filter by time"),
            &[
                ("0", t("全部时间", "all time")),
                ("30", t("近 30 天", "last 30 days")),
                ("7", t("近 7 天", "last 7 days")),
            ],
        );
        body.push_str("</div>");
        if desktop {
            let _ = write!(
                body,
                r#"<span class="subtle scan-status" id="scan-status" role="status">{}</span><button class="plain-btn" id="rescan" disabled>{}</button><button class="plain-btn review-primary" id="open-review" aria-haspopup="dialog">{}</button>"#,
                t("正在后台扫描…", "scanning in the background…"),
                t("重新扫描", "rescan"),
                t("AI 复核", "AI review"),
            );
        }
        let _ = write!(
            body,
            r#"</div></div><div class="content">
          <section data-pane="overview"><div class="intro"><h1>{}</h1><span class="subtle" id="undated"></span></div>
          <div class="metrics">
            <article class="metric"><span>{}</span><strong id="turns">—</strong></article>
            <article class="metric primary"><span>{}</span><strong id="hits">—</strong></article>
            <article class="metric"><span>{}</span><strong id="rate">—</strong></article>
            <article class="metric"><span>{}</span><strong id="active-days">—</strong></article>
          </div>
          <div class="panels"><section class="panel"><div class="panel-title"><h2>{}</h2><div class="segments" aria-label="{}">
            <button data-metric="turns" aria-pressed="true">{}</button><button data-metric="hits" aria-pressed="false">{}</button>
          </div></div><div id="legend" class="legend"></div><div class="plot"><div id="axis" class="axis" aria-hidden="true"></div><div id="bars" class="columns" role="group" aria-label="{}"></div></div>
          <div class="bottom-axis" aria-hidden="true"><span id="chart-start"></span><span id="chart-mid"></span><span id="chart-end"></span></div><div id="chart-detail" class="chart-detail"></div></section>
          <section class="panel"><div class="panel-title"><h2>{}</h2><div class="segments" aria-label="{}">
            <button data-group="source" aria-pressed="true">{}</button><button data-group="model" aria-pressed="false">{}</button>
          </div></div><div id="breakdown" class="breakdown"></div></section></div>
          <section class="panel heat-panel"><div class="panel-title"><h2>{}</h2><span id="heat-range" class="subtle"></span></div>
          <div class="heatmap"><div class="heat-grid" id="heat-grid" role="group" aria-label="{}"></div></div>
          <div class="heat-legend subtle">{} <i style="--cell:#e8edf4"></i><i style="--cell:#ccd9fa"></i><i style="--cell:#a4b8f5"></i><i style="--cell:#6c8ce9"></i><i style="--cell:#3e5dc7"></i> {} <span>· {}</span></div>
          <div id="heat-detail" class="heat-detail"></div></section></section>
          <section data-pane="records" hidden><div class="intro records-top"><div><h1>{}</h1><p class="subtle" id="record-count"></p></div><button class="plain-btn" id="clear-day" hidden></button></div>
          <div class="panel"><p id="empty-records" class="subtle" hidden></p><div id="record-list">"#,
            t("使用总览", "Overview"),
            t("真人消息", "Messages"),
            t("明确挨骂", "Insults"),
            t("挨骂占比", "Insult rate"),
            t("活跃天数", "Active days"),
            t("每日趋势", "Daily trend"),
            t("趋势指标", "trend metric"),
            t("真人消息", "Messages"),
            t("挨骂", "Insults"),
            t("选择日期", "select a day"),
            t("消息分布", "Message mix"),
            t("消息统计维度", "breakdown dimension"),
            t("工具", "Tool"),
            t("回复模型", "Model"),
            t("活跃日历", "Activity calendar"),
            t(
                "活跃日期，橙点表示当天有明确挨骂",
                "active days; an orange dot marks an explicit insult"
            ),
            t("消息量 少", "messages few"),
            t("多", "many"),
            t("橙点表示当天有挨骂", "an orange dot marks an insult"),
            t("挨骂记录", "Insults"),
        );
        if !desktop {
            body.push_str(&self.records_html());
        }
        let _ = write!(
            body,
            r#"</div></div></section>
          <section data-pane="index" hidden>
          <div class="intro"><h1>{}</h1><span class="subtle">{}</span></div>
          <div class="panel index-filters" aria-label="{}">
            <div class="index-field"><span>{}</span><div id="index-source-slot"></div></div>
            <div class="index-field"><span>{}</span><div id="index-range-slot"></div></div>
            <label class="index-field">{}<input id="index-query" type="search" placeholder="{}" autocomplete="off" spellcheck="false"></label>
            <div class="index-field"><span>{}</span>"#,
            t("被骂指数", "Insult index"),
            t(
                "每千条真人消息中的明确挨骂数",
                "explicit insults per thousand messages"
            ),
            t("被骂指数筛选", "insult index filters"),
            t("运行工具", "Tool"),
            t("时间范围", "Time range"),
            t("回复模型", "Model"),
            t("搜索模型", "search models"),
            t("最少消息", "Min messages"),
        );
        filter_markup(
            &mut body,
            "index-min",
            t("最少消息", "minimum messages"),
            &[
                ("0", t("不限", "any")),
                ("50", t("50 条", "50+")),
                ("100", t("100 条", "100+")),
                ("500", t("500 条", "500+")),
            ],
        );
        let _ = write!(
            body,
            r#"</div>
            <div class="index-filter-actions"><label><input id="index-hits-only" type="checkbox"> {}</label><button type="button" id="index-reset" class="plain-btn">{}</button></div>
          </div>
          <section class="panel index-chart-panel" id="index-chart">
            <div class="panel-title"><h2>{} <small id="index-chart-count"></small></h2>
              <div class="segments" aria-label="{}">
                <button type="button" data-index-metric="score" aria-pressed="true">{}</button>
                <button type="button" data-index-metric="hits" aria-pressed="false">{}</button>
                <button type="button" data-index-metric="turns" aria-pressed="false">{}</button>
              </div></div>
            <p class="subtle" id="index-chart-empty" hidden>{}</p>
            <div id="index-bars"></div>
          </section>
          <section class="panel index-panel">
            <div class="panel-title"><h2>{}</h2><span class="subtle" id="index-count" aria-live="polite"></span></div>
            <p id="index-empty" class="subtle" hidden>{}</p>
            <div id="index-table" class="index-scroll"><table aria-label="{}">
              <thead><tr>
                <th scope="col"><button type="button" data-index-sort="client">{}<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="model">{}<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="turns">{}<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="hits">{}<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="score">{}<span aria-hidden="true"></span></button></th>
              </tr></thead><tbody id="index-rows"></tbody>
            </table></div>
          </section></section>
          <section data-pane="sources" hidden><div class="intro"><h1>{}</h1><span class="subtle">{}</span></div>
          <div id="source-list" class="source-list"></div><div class="panel note"><p>{}</p><p class="subtle">{}</p></div></section>
          </div></main></div>"#,
            t("仅有挨骂", "insults only"),
            t("重置筛选", "reset filters"),
            t("组合对比", "Combination comparison"),
            t("对比指标", "comparison metric"),
            t("被骂指数", "Index"),
            t("明确挨骂", "Insults"),
            t("真人消息", "Messages"),
            t(
                "所选组合暂无可显示的被骂指数。",
                "no insult index to show for the selected combinations."
            ),
            t("组合明细", "Combination details"),
            t(
                "没有符合筛选条件的组合。",
                "no combination matches these filters."
            ),
            t(
                "按运行工具和回复模型统计被骂指数",
                "insult index by tool and model"
            ),
            t("运行工具", "Tool"),
            t("回复模型", "Model"),
            t("真人消息", "Messages"),
            t("明确挨骂", "Insults"),
            t("被骂指数", "Index"),
            t("数据来源", "Sources"),
            t("扫描覆盖 · 全部时间", "coverage · all time"),
            t(
                "真人消息按本机存档里的用户发送计数。Token 用量另计，包含输入上下文、模型生成和缓存读取。",
                "Messages count what you sent from the local archives. Token usage is counted separately and covers input context, model output and cache reads."
            ),
            t(
                "日期可以来自消息时间、同日相邻消息或文件日期。日期未知的消息计入全部时间。Codex 自动评审和子代理产生的消息不计入。",
                "Dates come from the message timestamp, a neighbouring message the same day, or the file date. Undated messages count towards all time. Codex auto-reviews and subagent messages are excluded."
            ),
        );
        if desktop {
            let mut provider = String::new();
            filter_markup(
                &mut provider,
                "review-provider",
                t("判定工具", "reviewer"),
                &[
                    ("codex", t("本地 Agent · Codex", "local agent · Codex")),
                    (
                        "claude",
                        t("本地 Agent · Claude Code", "local agent · Claude Code"),
                    ),
                    ("jev", t("Jev · TypeSafe 官方", "Jev · TypeSafe")),
                    (
                        "custom_system_one",
                        t("自定义 · Jev System One", "custom · Jev System One"),
                    ),
                    (
                        "custom_chat",
                        t("自定义 · OpenAI 兼容", "custom · OpenAI compatible"),
                    ),
                ],
            );
            let mut scope = String::new();
            filter_markup(
                &mut scope,
                "review-scope",
                t("复核范围", "review scope"),
                &[
                    (
                        "hits",
                        t("只复核规则命中的消息", "only messages matched by a rule"),
                    ),
                    (
                        "all",
                        t(
                            "重扫筛选范围内全部真人消息",
                            "re-check every message in the selected range",
                        ),
                    ),
                ],
            );
            body.push_str(&review_dialog(&provider, &scope));
        }
        let json = if desktop {
            "null".to_owned()
        } else {
            self.data()
                .to_string()
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .replace('&', "\\u0026")
        };
        page(&body, &json)
    }
}

fn date_basis_note(basis: &str) -> &'static str {
    match basis {
        "neighbors" => t("同日相邻消息补齐", "inferred from neighbouring messages"),
        "file" => t("按文件日期推断", "inferred from the file date"),
        _ => t("日期未知", "unknown date"),
    }
}

fn filter_markup(output: &mut String, id: &str, description: &str, options: &[(&str, &str)]) {
    let _ = write!(
        output,
        r#"<div class="filter" id="{id}-filter"><button type="button" class="filter-trigger" id="{id}-trigger" aria-label="{description}" aria-haspopup="listbox" aria-expanded="false" aria-controls="{id}-options"><span>{}</span></button><div class="filter-options" id="{id}-options" role="listbox" aria-label="{description}" hidden>"#,
        escape(options[0].1)
    );
    for (index, (value, label)) in options.iter().enumerate() {
        let _ = write!(
            output,
            r#"<button type="button" role="option" tabindex="-1" aria-selected="{}" data-value="{}">{}</button>"#,
            index == 0,
            escape(value),
            escape(label)
        );
    }
    output.push_str("</div></div>");
}

#[cfg(target_os = "macos")]
pub fn desktop_shell() -> String {
    Report {
        ruleset: detect::RULESET,
        sources: Vec::new(),
        hits: Vec::new(),
        rows: Vec::new(),
    }
    .render(true)
}

/// Strings the report document looks up at runtime. Keys must match `T.` in report.html.
fn dictionary() -> String {
    let views = serde_json::json!({
        "overview": t("总览", "Overview"),
        "records": t("挨骂记录", "Insults"),
        "index": t("被骂指数", "Insult index"),
        "sources": t("数据来源", "Sources"),
    });
    let map = serde_json::json!({
        "locale": i18n::lang().locale(),
        "nextLanguageCode": t("language:en", "language:zh"),
        "views": views,
        "unknownModel": t("模型未知", "unknown model"),
        "unknownDate": t("日期未知", "unknown date"),
        "dayDetail": t("{turns} 条真人消息 · {hits} 次挨骂", "{turns} messages · {hits} insults"),
        "dayDetailOne": t("{turns} 条真人消息 · {hits} 次挨骂", "{turns} messages · {hits} insult"),
        "insultOne": t("{count} 次挨骂", "{count} insult"),
        "months": if i18n::lang() == i18n::Lang::Zh {
            serde_json::json!(["1月","2月","3月","4月","5月","6月","7月","8月","9月","10月","11月","12月"])
        } else {
            serde_json::json!(["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"])
        },
        "viewRecords": t("查看记录", "view records"),
        "undated": t("{count} 条日期未知", "{count} messages without a date"),
        "noRecordsInRange": t("所选时间没有记录", "no records in this range"),
        "counting": t("正在统计…", "counting…"),
        "insultCount": t("{count} 次挨骂", "{count} insults"),
        "moreModels": t("另有 {count} 个模型", "{count} more models"),

        "lastDays": t("近 {days} 天", "last {days} days"),
        "last90": t("近 90 天", "last 90 days"),
        "countItems": t("{count} 条", "{count} messages"),
        "noRecords": t(
            "所选范围内没有明确冲 AI 的辱骂。",
            "no explicit insults towards the AI in this range.",
        ),
        "readingRecords": t("正在读取记录…", "reading records…"),
        "clearDay": t("{day} 清除日期", "{day} clear date"),
        "topGroups": t("前 {count} 组", "top {count}"),
        "hitsOverTurns": t("{hits} 次 / {turns} 条", "{hits} / {turns}"),
        "viewDetails": t("查看明细", "view details"),
        "noCombination": t(
            "没有符合筛选条件的组合。",
            "no combination matches these filters.",
        ),
        "combinations": t("{count} 个组合", "{count} combinations"),
        "optionLabel": t("{label}：{option}", "{label}: {option}"),
        "scanning": t("正在后台扫描…", "scanning in the background…"),
        "cachedScan": t("上次结果 · 后台更新中", "previous result · updating in the background"),
        "contributionSending": t("正在上报…", "Sending counts…"),
        "contributionWaiting": t("尚无成功上报", "No confirmed uploads yet"),
        "contributionConfirmed": t("近期已确认 {turns} 条消息 · {hits} 次挨骂", "Recently confirmed · {turns} messages · {hits} insults"),
        "contributionLastSync": t("最近成功 {time}", "Last successful upload {time}"),
        "endpointSystemOne": t("Jev System One 接口地址", "Jev System One endpoint"),
        "endpointChat": t("Chat Completions 接口地址", "Chat Completions endpoint"),
        "allSources": t("全部来源", "all sources"),
        "allTime": t("全部时间", "all time"),
        "reviewPreparing": t(
            "正在整理所选范围的本机会话…",
            "collecting local sessions in the selected range…",
        ),
        "reviewStopping": t("正在停止复核…", "stopping the review…"),
        "reviewProgress": t("正在复核 · {done} / {total} 条", "reviewing · {done} / {total}"),
        "reviewStopped": t("复核已停止", "review stopped"),
        "reviewFinished": t("复核完成 · {provider}", "review finished · {provider}"),
        "reviewSaved": t("复核完成 · 已保存结论", "review finished · verdicts saved"),
        "reviewSummary": t(
            "{reviewed} 条已复核 · {found} 条明确挨骂 · {uncertain} 条证据不足",
            "{reviewed} reviewed · {found} confirmed · {uncertain} uncertain",
        ),
        "reviewSkipped": t(
            "{count} 条没有可提交的脱敏片段",
            "{count} had no redacted excerpt to submit",
        ),
        "reviewAdded": t(
            "新增 {added} 条 · 规则命中获确认 {retained} / {reviewed} 条",
            "{added} new · {retained} of {reviewed} rule hits confirmed",
        ),
        "ruleHit": t("规则已命中", "matched by a rule"),
        "aiFound": t("AI 新发现", "found by the AI"),
        "sourceStatus": t("{turns} 条真人消息 · {hits} 次挨骂", "{turns} messages · {hits} insults"),
        "sourceStatusOne": t("{turns} 条真人消息 · {hits} 次挨骂", "{turns} messages · {hits} insult"),
        "noLocalRecords": t("未找到本地记录", "no local records found"),
        "sessionFiles": t("{count} 个会话文件", "{count} session files"),
        "compressedFiles": t("{count} 个压缩文件", "{count} compressed files"),
        "archivedFiles": t("{count} 个归档文件", "{count} archived files"),
        "historyTurns": t("{count} 条由输入历史补录", "{count} from input history"),
        "versionedTurns": t("{count} 条有工具版本", "{count} carry a tool version"),
        "autoReviews": t("{count} 个子代理或自动评审", "{count} subagent or auto-review messages"),
        "duplicateFiles": t("{count} 个重复会话文件", "{count} duplicate session files"),
        "readErrors": t("{count} 个读取异常", "{count} read errors"),
        "malformedLines": t("{count} 行无法解析", "{count} unparsed lines"),
        "inferredNeighbors": t(
            "{count} 条由同日相邻消息补齐",
            "{count} dated from neighbouring messages",
        ),
        "inferredFiles": t("{count} 条按文件日期推断", "{count} dated from the file"),
        "missingDays": t("{count} 条日期未知", "{count} without a date"),
    });
    map.to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

fn page(content: &str, data: &str) -> String {
    include_str!("report.html")
        .replace("<!-- MALEMA_LANG -->", i18n::lang().html())
        .replace("<!-- MALEMA_TITLE -->", t("骂了吗", "Malema"))
        .replace("<!-- MALEMA_I18N -->", &dictionary())
        .replace("<!-- MALEMA_DATA -->", data)
        .replace("<!-- MALEMA_BODY -->", content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_reviews_rebuild_local_counts_without_changing_uploaded_counts() {
        use crate::review_state::Verdict;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "malema-report-review-{}-{nonce}",
            std::process::id()
        ));
        let project = home.join(".claude/projects/test");
        fs::create_dir_all(&project).unwrap();
        let transcript = project.join("session.jsonl");
        let write_messages = |second: &str| {
            let messages = [
                "You are an idiot.",
                second,
                "Please review the task again.",
                "Your code is garbage.",
            ];
            let text: String = messages
                .iter()
                .enumerate()
                .map(|(index, message)| {
                    serde_json::json!({
                        "type": "user", "timestamp": format!("2026-01-01T12:00:0{index}Z"),
                        "message": {"role": "user", "content": message}
                    })
                    .to_string()
                        + "\n"
                })
                .collect();
            fs::write(&transcript, text).unwrap();
        };
        write_messages("That answer needs another pass.");

        let mut messages = Vec::new();
        let original = build_observed(&home, false, |source, turn, rule_hit| {
            messages.push((review_state::message_key(source, turn), rule_hit));
        });
        assert_eq!(original.summary()["hits"], 2);
        assert_eq!(
            messages.iter().map(|(_, hit)| *hit).collect::<Vec<_>>(),
            [true, false, false, true]
        );
        review_state::save(
            &home,
            &[
                (messages[0].0, Verdict::Excluded),
                (messages[1].0, Verdict::Output),
                (messages[2].0, Verdict::Direct),
            ],
        )
        .unwrap();

        let mut rule_hits = 0;
        let reviewed = build_observed(&home, false, |_, _, hit| rule_hits += usize::from(hit));
        assert_eq!(reviewed.summary()["hits"], 3);
        assert_eq!(reviewed.summary()["turns"], 4);
        assert_eq!(rule_hits, 2);
        assert!(reviewed.records_html().contains("ai.output-insult"));
        assert!(!reviewed.records_html().contains("You are an idiot."));
        assert_eq!(build(&home, false).summary()["hits"], 3);

        write_messages("I have changed my mind about the task.");
        assert_eq!(build(&home, false).summary()["hits"], 2);
        review_state::save(&home, &[(messages[2].0, Verdict::Excluded)]).unwrap();
        assert_eq!(build(&home, false).summary()["hits"], 1);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn desktop_shell_offers_the_language_switch_and_the_repository() {
        let shell = Report {
            ruleset: "test",
            sources: Vec::new(),
            hits: Vec::new(),
            rows: Vec::new(),
        }
        .render(true);
        assert!(shell.contains("id=\"language-toggle\""));
        assert!(shell.contains("id=\"open-repository\""));
        assert!(shell.contains("id=\"contribution-switch\""));
        assert!(shell.contains("id=\"contribution-status\""));
    }

    #[test]
    fn html_escapes_transcript_text() {
        assert_eq!(
            escape("<img src=x onerror='alert(1)'>&"),
            "&lt;img src=x onerror=&#39;alert(1)&#39;&gt;&amp;"
        );
    }
}
