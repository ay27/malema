# 骂了吗候选词表

## 底表

`base.json` 合并 LDNOOBW 的中文和英文表、dsojevic 的英文 JSON。合计 1,226 个去重词形，其中中文 318 个、英文 908 个。英文有 299 个词形来自两个来源。

- LDNOOBW 采用 CC BY 4.0。词表取自 [5faf2ba](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/tree/5faf2ba42d7b1c0977169ec3611df25a3c08eb13)。
- dsojevic/profanity-list 采用 MIT。词表取自 [c279243](https://github.com/dsojevic/profanity-list/tree/c27924319aa9bd6f917e3782b4f4b6604a50b652)。

`terms` 按语言、Unicode NFKC、大小写和空白归一后去重。每个词形保留来源和上游条目位置。使用 dsojevic 匹配式的最短拼写做去重。原始的星号语义、多写法、严重度、标签和例外保存在 `upstream_entries` 中。分发底表时保留两份许可文件和来源信息。

## Luna 补充

`luna-supplement.json` 保存 GPT‑5.6‑Luna max 根据完整公开底表补充的词形。模型生成了中文和英文各 60 组词及其变体。归一、去重和抽查后，保留中文 167 个、英文 220 个。底表和补充表合计 1,613 个词形。繁简、掩码和常见异写分别保留。

| 分类 | 含义 |
|---|---|
| `personal_insult` | 贬损对象本身的词或短语 |
| `artifact_insult` | 贬损回答、代码或其他产物的词或短语 |
| `vulgarity` | 粗口、强化语气或发泄 |
| `contextual` | 含义依赖语境的缩写、讽刺或固定说法 |

## 本机样本复核

| 候选来源 | 挨骂样本的词面命中 | 非挨骂样本的词面命中 |
|---|---:|---:|
| 底表 | 23 / 32 | 18 / 206 |
| 底表加 Luna 补充 | 23 / 32 | 43 / 206 |

沿用此前已标注的 238 条可判定样本。按归一后的字面词形匹配，英文加单词边界。本次样本仅用于本机复核。补充后多命中的 25 条非挨骂样本中，有 24 条命中「卧槽」。

此前 zh-en-3 规则漏掉的 8 条挨骂样本中，底表找到了 5 条词面线索。字面命中用于提出候选，计入挨骂还需确认说话对象是当前 AI 或它的产出。zh-en-4 将公开底表和 Luna 的人身、产物辱骂候选接入既有定向句式。
