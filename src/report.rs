use crate::detect::{self, Hit};
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
    let mut hits = Vec::new();
    let mut totals = BTreeMap::<_, Count>::new();
    let sources = sources::scan(home, use_env, |stats, turn| {
        let hit = detect::detect(&turn.text);
        observe(stats.source, &turn, hit.is_some());
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
                    Some(InferredDay::SameDayNeighbors) => Some("同日相邻消息补齐"),
                    Some(InferredDay::FileMetadata) => Some("按文件日期推断"),
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
            let visible_date = match entry.date_basis {
                Some(basis) => format!("{date}（{basis}）"),
                None => entry.date.as_deref().unwrap_or("日期未知").to_owned(),
            };
            let model = escape(entry.model.as_deref().unwrap_or("模型未知"));
            let session = escape(&entry.session);
            let evidence = escape(&entry.hit.evidence);
            let excerpt = escape(&entry.excerpt);
            let reason = entry.hit.reason;
            let code = entry.hit.rule;
            let position = if entry.line == 0 {
                "Cursor 会话数据库".to_string()
            } else {
                format!("第 {} 行", entry.line)
            };
            let _ = write!(
                html,
                "<details class='record' data-client='{source}' data-date='{date}'><summary><span><strong>{evidence}</strong><small>{source_label} · {model} · {visible_date}</small></span></summary><div class='record-body'><p><b>为什么计入</b>　{reason}</p><blockquote>{excerpt}</blockquote><small>{session} · {position} · {code}</small></div></details>"
            );
        }
        html
    }

    pub fn render(&self, desktop: bool) -> String {
        let mut body = format!(
            r#"<div class="shell"><aside><div class="brand"><img src="data:image/png;base64,{}" width="32" height="32" alt="">骂了吗</div><nav aria-label="报表导航">
          <button data-tab="overview" aria-current="page"><span class="nav-icon" aria-hidden="true">◫</span> 总览</button>
          <button data-tab="records"><span class="nav-icon" aria-hidden="true">≡</span> 挨骂记录</button>
          <button data-tab="index"><span class="nav-icon" aria-hidden="true">▥</span> 被骂指数</button>
          <button data-tab="sources"><span class="nav-icon" aria-hidden="true">▤</span> 数据来源</button>
          "#,
            include_str!("../assets/brand-mark.base64").trim(),
        );
        if desktop {
            body.push_str(r#"<button id="open-review-nav" aria-haspopup="dialog"><span class="nav-icon" aria-hidden="true">✓</span> AI 复核</button>"#);
            body.push_str(r#"<button id="open-leaderboard" type="button" title="在默认浏览器打开"><span class="nav-icon" aria-hidden="true">↗</span> 全球榜单</button>"#);
        }
        body.push_str("</nav>");
        if desktop {
            body.push_str(r#"<div class="contribution-control">
              <button class="contribution-switch" id="contribution-switch" type="button" role="switch" aria-checked="false" aria-label="匿名提交，参与全球排行" title="开启后自动提交匿名统计，首次包含最近 7 天" disabled>
                <span class="contribution-label">匿名提交<small>参与全球排行</small></span><span class="switch-track" aria-hidden="true"></span>
              </button>
              <p class="contribution-error" id="contribution-error" role="status" hidden></p>
            </div>"#);
        } else {
            body.push_str(r#"<div class="local">本机离线</div>"#);
        }
        body.push_str(
            r#"</aside><main>
          <div class="toolbar"><strong id="current-title">总览</strong><div class="controls"><div id="toolbar-filters">
          "#,
        );
        let sources: Vec<_> = Source::ALL
            .iter()
            .map(|source| (source.label(), source.label()))
            .collect();
        let mut source_choices = vec![("", "全部来源")];
        source_choices.extend(sources);
        filter_markup(&mut body, "source", "筛选来源", &source_choices);
        filter_markup(
            &mut body,
            "range",
            "筛选时间",
            &[("0", "全部时间"), ("30", "近 30 天"), ("7", "近 7 天")],
        );
        body.push_str("</div>");
        if desktop {
            body.push_str(r#"<span class="subtle scan-status" id="scan-status" role="status">正在后台扫描…</span><button class="plain-btn" id="rescan" disabled>重新扫描</button><button class="plain-btn review-primary" id="open-review" aria-haspopup="dialog">AI 复核</button>"#);
        }
        body.push_str(r#"</div></div><div class="content">
          <section data-pane="overview"><div class="intro"><h1>使用总览</h1><span class="subtle" id="undated"></span></div>
          <div class="metrics">
            <article class="metric"><span>真人消息</span><strong id="turns">—</strong></article>
            <article class="metric primary"><span>明确挨骂</span><strong id="hits">—</strong></article>
            <article class="metric"><span>挨骂占比</span><strong id="rate">—</strong></article>
            <article class="metric"><span>活跃天数</span><strong id="active-days">—</strong></article>
          </div>
          <div class="panels"><section class="panel"><div class="panel-title"><h2>每日趋势</h2><div class="segments" aria-label="趋势指标">
            <button data-metric="turns" aria-pressed="true">真人消息</button><button data-metric="hits" aria-pressed="false">挨骂</button>
          </div></div><div id="legend" class="legend"></div><div class="plot"><div id="axis" class="axis" aria-hidden="true"></div><div id="bars" class="columns" role="group" aria-label="选择日期"></div></div>
          <div class="bottom-axis" aria-hidden="true"><span id="chart-start"></span><span id="chart-mid"></span><span id="chart-end"></span></div><div id="chart-detail" class="chart-detail"></div></section>
          <section class="panel"><div class="panel-title"><h2>消息分布</h2><div class="segments" aria-label="消息统计维度">
            <button data-group="source" aria-pressed="true">工具</button><button data-group="model" aria-pressed="false">回复模型</button>
          </div></div><div id="breakdown" class="breakdown"></div></section></div>
          <section class="panel heat-panel"><div class="panel-title"><h2>活跃日历</h2><span id="heat-range" class="subtle"></span></div>
          <div class="heatmap"><div class="heat-grid" id="heat-grid" role="group" aria-label="活跃日期，橙点表示当天有明确挨骂"></div></div>
          <div class="heat-legend subtle">消息量 少 <i style="--cell:#e8edf4"></i><i style="--cell:#ccd9fa"></i><i style="--cell:#a4b8f5"></i><i style="--cell:#6c8ce9"></i><i style="--cell:#3e5dc7"></i> 多 <span>· 橙点表示当天有挨骂</span></div>
          <div id="heat-detail" class="heat-detail"></div></section></section>
          <section data-pane="records" hidden><div class="intro records-top"><div><h1>挨骂记录</h1><p class="subtle" id="record-count"></p></div><button class="plain-btn" id="clear-day" hidden></button></div>
          <div class="panel"><p id="empty-records" class="subtle" hidden></p><div id="record-list">"#);
        if !desktop {
            body.push_str(&self.records_html());
        }
        body.push_str(r#"</div></div></section>
          <section data-pane="index" hidden>
          <div class="intro"><h1>被骂指数</h1><span class="subtle">每千条真人消息中的明确挨骂数</span></div>
          <div class="panel index-filters" aria-label="被骂指数筛选">
            <div class="index-field"><span>运行工具</span><div id="index-source-slot"></div></div>
            <div class="index-field"><span>时间范围</span><div id="index-range-slot"></div></div>
            <label class="index-field">回复模型<input id="index-query" type="search" placeholder="搜索模型" autocomplete="off" spellcheck="false"></label>
            <div class="index-field"><span>最少消息</span>"#);
        filter_markup(
            &mut body,
            "index-min",
            "最少消息",
            &[
                ("0", "不限"),
                ("50", "50 条"),
                ("100", "100 条"),
                ("500", "500 条"),
            ],
        );
        body.push_str(r#"</div>
            <div class="index-filter-actions"><label><input id="index-hits-only" type="checkbox"> 仅有挨骂</label><button type="button" id="index-reset" class="plain-btn">重置筛选</button></div>
          </div>
          <section class="panel index-chart-panel" id="index-chart">
            <div class="panel-title"><h2>组合对比 <small id="index-chart-count"></small></h2>
              <div class="segments" aria-label="对比指标">
                <button type="button" data-index-metric="score" aria-pressed="true">被骂指数</button>
                <button type="button" data-index-metric="hits" aria-pressed="false">明确挨骂</button>
                <button type="button" data-index-metric="turns" aria-pressed="false">真人消息</button>
              </div></div>
            <p class="subtle" id="index-chart-empty" hidden>所选组合暂无可显示的被骂指数。</p>
            <div id="index-bars"></div>
          </section>
          <section class="panel index-panel">
            <div class="panel-title"><h2>组合明细</h2><span class="subtle" id="index-count" aria-live="polite"></span></div>
            <p id="index-empty" class="subtle" hidden>没有符合筛选条件的组合。</p>
            <div id="index-table" class="index-scroll"><table aria-label="按运行工具和回复模型统计被骂指数">
              <thead><tr>
                <th scope="col"><button type="button" data-index-sort="client">运行工具<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="model">回复模型<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="turns">真人消息<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="hits">明确挨骂<span aria-hidden="true"></span></button></th>
                <th scope="col"><button type="button" data-index-sort="score">被骂指数<span aria-hidden="true"></span></button></th>
              </tr></thead><tbody id="index-rows"></tbody>
            </table></div>
          </section></section>
          <section data-pane="sources" hidden><div class="intro"><h1>数据来源</h1><span class="subtle">扫描覆盖 · 全部时间</span></div>
          <div id="source-list" class="source-list"></div><div class="panel note"><p>真人消息按本机存档里的用户发送计数。Token 用量另计，包含输入上下文、模型生成和缓存读取。</p><p class="subtle">日期可以来自消息时间、同日相邻消息或文件日期。日期未知的消息计入全部时间。Codex 自动评审和子代理产生的消息不计入。</p></div></section>
          </div></main></div>"#);
        if desktop {
            let mut provider = String::new();
            filter_markup(
                &mut provider,
                "review-provider",
                "判定工具",
                &[
                    ("codex", "本地 Agent · Codex"),
                    ("claude", "本地 Agent · Claude Code"),
                    ("jev", "Jev · TypeSafe 官方"),
                    ("custom_system_one", "自定义 · Jev System One"),
                    ("custom_chat", "自定义 · OpenAI 兼容"),
                ],
            );
            let mut scope = String::new();
            filter_markup(
                &mut scope,
                "review-scope",
                "复核范围",
                &[
                    ("hits", "只复核规则命中的消息"),
                    ("all", "重扫筛选范围内全部真人消息"),
                ],
            );
            body.push_str(
                &include_str!("review.html")
                    .replace("<!-- REVIEW_PROVIDER -->", &provider)
                    .replace("<!-- REVIEW_SCOPE -->", &scope),
            );
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

fn page(content: &str, data: &str) -> String {
    include_str!("report.html")
        .replace("<!-- MALEMA_DATA -->", data)
        .replace("<!-- MALEMA_BODY -->", content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escapes_transcript_text() {
        assert_eq!(
            escape("<img src=x onerror='alert(1)'>&"),
            "&lt;img src=x onerror=&#39;alert(1)&#39;&gt;&amp;"
        );
    }
}
