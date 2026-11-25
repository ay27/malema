use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

pub const RULESET: &str = "zh-en-4";

#[derive(Debug, Serialize)]
pub struct Hit {
    pub rule: &'static str,
    pub reason: String,
    pub evidence: String,
}

struct Rule {
    id: &'static str,
    pattern: Regex,
}

fn rule(id: &'static str, pattern: &str) -> Rule {
    Rule {
        id,
        pattern: RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .expect(id),
    }
}

/// Human readable explanation of a rule id, in the active interface language.
pub fn reason(id: &str) -> String {
    let (zh, en) = match id {
        "zh.direct-insult" => ("用辱骂词直接称呼 AI", "calls the AI a slur directly"),
        "zh.direct-insult-adjective" => (
            "用笨、蠢等措辞直接贬损 AI",
            "calls the AI dumb or stupid directly",
        ),
        "zh.direct-curse" => (
            "用第二人称加粗口直接斥责 AI",
            "curses at the AI in the second person",
        ),
        "zh.profanity-command" => (
            "用粗口直接命令 AI 停止或修改操作",
            "orders the AI to stop or change something with profanity",
        ),
        "zh.profanity-rebuke" => (
            "用粗口直接反问或斥责 AI",
            "rebukes the AI with profanity in a direct question",
        ),
        "zh.profanity-directed-action" => (
            "用粗口接直接反问或修正命令",
            "follows profanity with a direct demand aimed at the AI",
        ),
        "zh.profanity-ai-artifact" => (
            "用明确辱骂词描述 AI 刚做的操作或调用的模型",
            "insults the action or model the AI just produced",
        ),
        "zh.direct-dismissal" => (
            "对 AI 使用明确的辱骂式赶斥",
            "dismisses the AI with an explicit slur",
        ),
        "zh.output-insult" => (
            "用辱骂词指向 AI 写的代码或答案",
            "aims a slur at the code or answer the AI wrote",
        ),
        "zh.output-quality-insult" => (
            "用明确辱骂词贬损 AI 产物的质量",
            "insults the quality of what the AI produced",
        ),
        "zh.output-creation-insult" => (
            "用明确辱骂词描述 AI 刚生成的产物",
            "describes the AI output with an explicit slur",
        ),
        "en.direct-insult" => (
            "用英文辱骂词直接评价 AI",
            "insults the AI directly in English",
        ),
        "en.direct-curse" => (
            "用明确的英文粗口斥责 AI",
            "swears at the AI with explicit English profanity",
        ),
        "en.output-insult" => (
            "用英文辱骂词指向 AI 的代码或答案",
            "aims an English slur at the AI's code or answer",
        ),
        "mixed.direct-insult" => (
            "中英混写仍明确用辱骂词称呼 AI",
            "calls the AI a slur while mixing Chinese and English",
        ),
        _ => ("规则命中", "matched a rule"),
    };
    crate::i18n::pick(zh, en).to_owned()
}

// Curated terms are only candidates inside a rule with an identified target.
const ZH_INSULT: &str = r"(?:傻[逼比b]|煞笔|沙比|蠢[货貨猪豬]|笨蛋|廢物|废物|廢柴|废柴|飯桶|饭桶|腦殘|脑残|弱智|智障|白痴|蠢蛋|蠢材|垃圾|神经病|神經病|智障玩意[儿兒]?|傻狗)";
const ZH_ARTIFACT: &str = r"(?:代码|代碼|回答|答案|结果|結果|方案|输出|輸出|实现|實現|设计|設計|报表|報表|图表|圖表|补丁|補丁|操作|文案|告警)";
const ZH_OUTPUT_INSULT: &str = r"(?:垃圾|狗屎|狗屁|傻[逼比b]|智障|脑残|腦殘)";
const EN_INSULT: &str = r"(?:idiot|moron|imbecile|dumbass|asshole|dipshit|stupid|incompetent|useless|piece of shit|waste of space|garbage|trash)";
const EN_OUTPUT_INSULT: &str = r"(?:garbage|trash|bullshit|shit|a\s+piece\s+of\s+shit|stupid)";
const EN_MOD: &str = r"(?:(?:a|an|the|such|absolute|complete|total|fucking|fcking|really|just|goddamn|so|bloody)\s+){0,4}";

#[derive(Deserialize)]
struct Lexicon {
    terms: Vec<Term>,
}

#[derive(Deserialize)]
struct Term {
    language: String,
    text: String,
    category: Option<String>,
}

static LEXICON: LazyLock<Vec<Term>> = LazyLock::new(|| {
    [
        include_str!("../lexicon/base.json"),
        include_str!("../lexicon/luna-supplement.json"),
    ]
    .into_iter()
    .flat_map(|json| {
        serde_json::from_str::<Lexicon>(json)
            .expect("bundled lexicon")
            .terms
    })
    .collect()
});

fn extended(pattern: &str, language: &str, category: &str) -> String {
    let mut terms: Vec<_> = LEXICON
        .iter()
        .filter(|term| {
            term.language == language && term.category.as_deref().unwrap_or(category) == category
        })
        .map(|term| term.text.as_str())
        .collect();
    terms.sort_unstable_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    terms.dedup();
    let mut expanded = pattern.strip_suffix(')').expect("alternation").to_owned();
    for term in terms {
        expanded.push('|');
        expanded.push_str(&regex::escape(term));
    }
    expanded.push(')');
    expanded
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let zh_insult = extended(ZH_INSULT, "zh", "personal_insult");
    let zh_output = extended(ZH_OUTPUT_INSULT, "zh", "artifact_insult");
    let en_insult = extended(EN_INSULT, "en", "personal_insult");
    let en_output = extended(EN_OUTPUT_INSULT, "en", "artifact_insult");
    vec![
        rule(
            "zh.direct-insult",
            &format!(
                r"(?:^|[，,\s])(?:你(?:[这這](?:个|個)?|(?:就是|也是|简直是|簡直是|真是|是不是|是)(?:[个個]|一[个個])?|也太|太|真(?:的)?|个|個)?)\s*(?:[个個]|一[个個])?\s*{zh_insult}(?:[吧啊呀吗嗎了欸诶喔]|[，,\s]|$)"
            ),
        ),
        rule(
            "zh.direct-insult-adjective",
            r"(?:^|[，,\s])(?P<evidence>(?:感觉|感覺|我觉得|我覺得)?你(?:(?:好|真(?:的)?|也太|太|很|(?:怎么|怎麼|咋)(?:能|会|會)?(?:变|變)?(?:得|这么|這麼|那么|那麼)?)(?:笨|蠢|愚蠢)|(?:再(?:这么|這麼)?|又|一直在|别(?:再)?|別(?:再)?|(?:怎么|怎麼|为什么|為什麼)(?:还|還|总|總)?)(?:这么|這麼|那么|那麼)?犯蠢)(?:[啊呀了欸吧吗嗎呢]{0,2}))(?:[，,\s]|$)",
        ),
        rule(
            "zh.direct-curse",
            r"(?:^|[，,\s])(?P<evidence>你(?:他[妈媽](?:的)?|[tT][mM](?:[dD])?|特[么麼]的)|(?:我|老子)?[操艹肏]你(?:[妈媽]|[大你]?爷|祖宗)?)(?:$|[，,\s]|[\u{3400}-\u{9fff}])",
        ),
        rule(
            "zh.profanity-command",
            r"(?:^|[，,\s])(?P<evidence>(?:(?:你(?:就|也|还|還|再|能不能)?)|能不能|可不可以)?别他[妈媽](?:的)?(?:\s*[\p{Han}A-Za-z_]{1,8})?)",
        ),
        rule(
            "zh.profanity-rebuke",
            r"(?:^|[，,\s])(?P<evidence>我(?:他[妈媽]的?|[tT][mM](?:[dD])?)\s*(?:啥时候|啥時候|什么时候|什麼時候|哪时候|哪時候|何时|何時)\s*(?:跟你|跟妳|给你|給你|告诉你|告訴你)(?:说|說|说过|說過)?|他[妈媽]的?[，,\s]*你(?:管[他它这這]|在[干幹]嘛|搞[什啥]么)|你[妈媽]的[啊呀吧]?(?:$|[，,\s]))",
        ),
        rule(
            "zh.profanity-directed-action",
            r"(?:^|[，,\s])(?P<evidence>你[妈媽]的\s*(?:能不能|可不可以|给我|給我|你(?:这|這|那|怎么|怎麼|又|到底)|到底)|他[妈媽娘]的?\s*(?:赶紧给我|趕緊給我|给我|給我)\s*(?:修好|改好|改回来|改回來|改对|改對|撤掉|停下)|他[妈媽娘]的?[，,\s]+(?:[^，,。；;]{1,28}[，,]\s*)?你管[他它这這那]干嘛)",
        ),
        rule(
            "zh.profanity-ai-artifact",
            r"(?:^|[，,\s])(?P<evidence>你(?:还|還|又|居然|竟然)(?:给我|給我)?\s*(?:单独|單獨|特意|擅自|偏要)?(?:写|寫|搞|改|生成|做)[^，,。；;]{0,35}(?:傻[逼比b]|智障|脑残|腦殘|狗屎)的?(?:操作|代码|代碼|方案|设计|設計|回答|实现|實現)|[这這](?:个|個)?傻[逼比b]模型[，,\s]+你)(?:$|[，,\s]|[\p{Han}])",
        ),
        rule(
            "zh.direct-dismissal",
            r"^(?:你(?:给我|給我)?)?(?:滚蛋|滾蛋|滚出去|滾出去|滚开|滾開|去死)(?:吧|啊|呀|[，,\s]|$)",
        ),
        rule(
            "zh.output-insult",
            r"(?:你(?:刚|剛|刚才|剛才)?(?:写|寫|生成|给|給|做|改|出的|交)(?:出来|出來|出)?的?(?:这|這|那)?(?:一)?(?:段|份|套|堆|坨)?(?:代码|代碼|回答|答案|结果|結果|东西|東西|方案|输出|輸出|实现|實現)?(?:\s|，|,)*(?:就是|简直是|簡直是|全是|真是|是)(?:一|一[个個])?(?:堆|坨)?(?:垃圾|狗屎|狗屁|废物|廢物|屎)(?:[啊呀吧了]|[,，\s]|$)|你(?:怎么|怎麼)写了(?:一)?(?:堆|坨)(?:垃圾|狗屎)(?:[代码代碼]|$)|你(?:又)?写了(?:一)?(?:堆|坨)(?:垃圾代码|垃圾代碼|狗屎代码|狗屎代碼))",
        ),
        rule(
            "zh.output-quality-insult",
            &format!(
                r"(?:^|[，,\s])(?P<evidence>你(?:的|[这這那](?:个|個|份|版|段|套)?)\s*{ZH_ARTIFACT}(?:风格|風格|质量|品質|質量|效果|写法|寫法)?\s*(?:真(?:的)?|很|太|也太|实在|實在|简直|簡直|就是|是|得){{0,3}}\s*(?:{zh_output}|烂透|爛透|一坨屎)[啊呀了吧呢嗎吗]{{0,2}})(?:[，,\s]|$)"
            ),
        ),
        rule(
            "zh.output-creation-insult",
            &format!(
                r"(?:^|[，,\s]|没想到|沒想到)(?P<evidence>你(?:竟然|居然|又|还|還|能|给我|給我|怎么|怎麼){{0,3}}(?:写|寫|生成|输出|輸出|做|给|給)(?:出来|出來|出|了){{0,2}}(?:(?:这么|這麼|那么|那麼|这种|這種)?{zh_output}的?{ZH_ARTIFACT}|(?:一)?(?:堆|坨)(?:狗屎|屎)|的(?:是|就是)(?:一)?(?:个|個)?(?:啥|什么|甚麼)?(?:一)?(?:个|個)?破玩意[儿兒]?)[啊呀了吧呢嗎吗]{{0,2}})(?:[，,\s]|$)"
            ),
        ),
        rule(
            "en.direct-insult",
            &format!(
                r"(?:\byou(?:\s+are|'re|\s+sound|\s+seem)\s+{EN_MOD}|\bare\s+you\s+{EN_MOD}|\byou\s+(?:you\s+)?{EN_MOD}){en_insult}\b"
            ),
        ),
        rule(
            "en.direct-curse",
            r"(?:^|[,\s])(?:(?:go\s+)?fuck\s+(?:you|yourself)|screw\s+you|what\s+the\s+fuck\s+are\s+you\s+doing)(?:\s|[,!]|$)",
        ),
        rule(
            "en.output-insult",
            &format!(
                r"\b(?:your\s+(?:code|answer|reply|output|patch|solution|implementation)|(?:the|this)\s+(?:code|answer|patch|implementation)\s+you\s+(?:wrote|gave me|generated))\s+(?:is|was)\s+{EN_MOD}{en_output}\b"
            ),
        ),
        rule(
            "mixed.direct-insult",
            &format!(
                r"(?:^|[，,\s])你(?:[这這][个個]?|(?:就是|真是|是)(?:[个個]|一[个個])?|[个個])\s*{EN_MOD}{en_insult}\b"
            ),
        ),
    ]
});

static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`+[^`\r\n]*`+").unwrap());
static QUOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"“[^”\n]*”|‘[^’\n]*’|「[^」\n]*」|『[^』\n]*』|"[^"\n]*""#).unwrap()
});
static SENTENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[;。！？!?；]+|\.\s+|\.$").unwrap());
static POSITIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?i)(?:牛[逼b批比]|牛啊|太强了|太強了|太棒了|太好了|太厉害了|太厲害了|真厉害|真厲害|真棒|真强|真強|天才|好厉害|好厲害|fucking\s+(?:great|awesome|amazing|brilliant)|show(?:ing)?\s+(?:me\s+)?(?:an?|the)\s+example)"
).unwrap()
});
static REPORTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r#"(?i)(?:举个例|舉個例|例如|假设|假設|比如|(?:他|她|对方|對方|用户|用戶)(?:刚才|剛才|当时|當時)?(?:说|說|写|寫|骂|罵)|原文|原话|原話|示例|这句话|這句話|那句话|那句話|翻译|翻譯|他骂|他罵|她骂|她罵|对你说|對你說|我刚说|我剛說|\b(?:translate|example|quotation|quote|diagnose|lyrics|verbatim)\b|\b(?:says|said|wrote|read|called)\s*[:“"']|\b(?:he|she|they|someone|people|the user)\s+(?:says|said|wrote|called)\b|\b(?:stop|don't|never)\s+saying\b|\b(?:not|never)\s+(?:calling|called|saying)\b|\b(?:don't|do\s+not|wouldn't|would\s+not)\s+(?:call|say)\b|\b(?:call|called)\s+you\s+an?\b)"#
).unwrap()
});
static ENGINEERING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?i)\bgarbage\s+(?:collection|collector|collecting|collected)\b|垃圾(?:回收|收集|清理|分类|分類|桶|箱|站)"
).unwrap()
});

pub(crate) fn visible_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut fence: Option<char> = None;
    text.lines().filter_map(move |line| {
        let line = line.trim();
        if let Some(marker) = line.chars().next().filter(|c| *c == '`' || *c == '~') {
            if line.chars().take_while(|c| *c == marker).count() >= 3 {
                if fence == Some(marker) {
                    fence = None;
                } else if fence.is_none() {
                    fence = Some(marker);
                }
                return None;
            }
        }
        (!line.is_empty()
            && fence.is_none()
            && !line.starts_with('>')
            && !line.starts_with('<')
            && !line.starts_with("at ")
            && !line.starts_with("INFO ")
            && !line.starts_with("DEBUG ")
            && !line.starts_with("ERROR ")
            && !line.starts_with("WARN "))
        .then_some(line)
    })
}

pub fn detect(text: &str) -> Option<Hit> {
    for line in visible_lines(text) {
        let code_removed = INLINE_CODE.replace_all(line, " ");
        let plain = QUOTE.replace_all(&code_removed, " ");
        let normalized = plain.nfkc().collect::<String>();
        for clause in SENTENCE.split(&normalized) {
            if REPORTED.is_match(clause) || POSITIVE.is_match(clause) {
                continue;
            }
            let clean_clause = ENGINEERING.replace_all(clause, " ");
            for rule in RULES.iter() {
                if let Some(captures) = rule.pattern.captures(clean_clause.trim()) {
                    let hit = captures
                        .name("evidence")
                        .or_else(|| captures.get(0))
                        .expect("matched rule");
                    return Some(Hit {
                        rule: rule.id,
                        reason: reason(rule.id),
                        evidence: hit
                            .as_str()
                            .trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | '，'))
                            .to_owned(),
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_counts_directed_insults() {
        assert_eq!(detect("服了，你他妈不更新吗。").unwrap().evidence, "你他妈");
        for input in [
            "你就是个废物",
            "你他妈的能不能先读我的要求",
            "你写的这段就是垃圾",
            "Fuck you.",
            "You are an idiot.",
            "You're a fucking idiot.",
            "You idiot.",
            "Your code is garbage.",
            "你是个 fucking idiot。",
            "都说了，你别他妈混入其他数据。",
            "别他妈检查了行不行。",
            "我他妈啥时候给你说过要跑这个任务。",
            "你还给我搞了个先建再删的傻逼操作。",
            "这傻逼模型，你给我记住。",
            "他妈的，你管他干嘛。",
            "你妈的啊。",
        ] {
            assert!(detect(input).is_some(), "{input}");
        }
        for input in [
            "你把垃圾回收那段改一下",
            "他妈的，这个 SDK 真难用",
            "卧槽，这个效果也太好了",
            "你为什么又改错文件了",
            "把“你他妈就是废物”翻成英文",
            "This is fucking awesome.",
            "Can you fix garbage collection?",
            "The SDK is fucking unusable. Can you replace it?",
            "The error message says `you are an idiot`.",
            "Please fix this fucking SDK.",
            "Great job deleting prod again.",
            "他刚才说 you are an idiot.",
            "Your code is garbage collection logic.",
            "I am not calling you stupid; this dataset has a bad label.",
            "I'm not saying you're an idiot.",
            "I would not call you stupid.",
            "你的设计文档里用过“别他妈检查”这种词吗",
            "我他妈很心累啊。",
            "你帮我改这个傻逼 SDK 的配置。",
            "他妈的，今天这个 SDK 又炸了。",
            "你妈的生日是哪一天。",
            "前面的模型是个傻逼，你帮我看下它留下的文件。",
            "代码例子:\n```text\nyou are an idiot\n```",
            "> fuck you",
            "你他妈真牛逼",
        ] {
            assert!(detect(input).is_none(), "{input}");
        }
    }

    #[test]
    fn recognizes_direct_insults_in_additional_sentence_forms() {
        for (input, expected_rule) in [
            ("你这版答案简直烂透了", "zh.output-quality-insult"),
            ("没想到你能输出这么垃圾的方案", "zh.output-creation-insult"),
            ("你給我寫了一坨屎", "zh.output-creation-insult"),
            ("你写的是个啥破玩意", "zh.output-creation-insult"),
            ("感觉你很愚蠢啊", "zh.direct-insult-adjective"),
            ("你别再这么犯蠢了", "zh.direct-insult-adjective"),
            ("你妈的能不能按要求改", "zh.profanity-directed-action"),
            ("他妈的赶紧给我修好啊", "zh.profanity-directed-action"),
        ] {
            assert_eq!(detect(input).unwrap().rule, expected_rule, "{input}");
        }
        for input in [
            "你的代码质量需要再提高一点",
            "你生成了用于垃圾回收的代码吗",
            "你写了个垃圾分类方案吗",
            "你这版方案面向垃圾桶的维护",
            "别把你自己的感觉归罪给那个很笨的工具",
            "有人觉得你好笨，但我不这么认为",
            "他妈的，今天这个服务又挂了，给我查下原因",
            "这份来自第三方的错误报告很垃圾，你能修改格式吗",
            "假设你输出这么垃圾的结果，测评会怎么打分？",
            "把『你写的是个啥破玩意』翻译一下",
        ] {
            assert!(detect(input).is_none(), "{input}");
        }
    }
}
