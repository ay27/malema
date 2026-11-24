# 骂了吗

macOS 本地会话报表。双击「骂了吗.app」开始扫描。总览支持日期和工具筛选，趋势和活跃日历可以逐日查看。点击「挨骂记录」中的任意一条，查看命中的原始行片段、原因和规则编号。没有可用回复模型的记录显示「模型未知」。

## 会话来源

| 来源 | 默认位置 |
| --- | --- |
| Codex | `~/.codex/sessions`、`~/.codex/archived_sessions`，含 `.jsonl.zst` 压缩存档 |
| Claude Code | `~/.claude/projects`；对没有会话文件的条目尝试从 `history.jsonl` 补录 |
| DeepSeek Harness | `~/.dsh/sessions`，兼容 v4 和早期 JSONL / Zstandard |
| WorkBuddy | `~/.workbuddy/projects` |
| Cursor | `~/.cursor/projects/*/agent-transcripts`；另支持 macOS 全局会话数据库的本地消息 |
| pi | `~/.pi/agent/sessions` |

Codex 支持 `CODEX_HOME`。Claude Code 支持 `CLAUDE_CONFIG_DIR`。pi 支持 `PI_CODING_AGENT_DIR` 和 `PI_CODING_AGENT_SESSION_DIR`。传入 `--home DIR` 可对夹具或其他会话根目录离线测试。

打开桌面版时先显示报表和上次缓存的统计，再在后台读取这些来源。点击「重新扫描」也会在后台更新。缓存以当前用户专用权限保存本地统计，命中记录的片段在扫描后加载到窗口。命令行显式导出的报表也含私人会话片段，在 Unix 上以当前用户专用权限写入。

Cursor 日历优先用消息时间。同一天的相邻消息能补齐缺失日期；文件创建与修改都在同一天、且与已有消息日期一致时，记录标为「按文件日期推断」。剩余日期不明的消息保留在全部时间总数。

## 本地构建

需要 Rust 1.93 和 macOS Command Line Tools。桌面窗口使用系统 WKWebView。

```sh
cargo test --locked
cargo build --locked --release
./scripts/package-macos.sh target/release/malema 骂了吗.app
```

命令行示例。

```sh
target/release/malema --summary
target/release/malema --report ./私人报表.html
```

## macOS 发版

推送 `v<版本>` 标签后，Release 工作流构建 Apple Silicon 安装包。标签应与
`Cargo.toml` 的 `package.version` 一致。每次正式发版递增
`package.metadata.malema.bundle-version`。更新器依此判断新旧构建。

| GitHub Actions 配置 | 值 |
| --- | --- |
| Secret `CSC_LINK` | Base64 编码的 Developer ID Application P12 |
| Secret `CSC_KEY_PASSWORD` | P12 导出密码 |
| Secrets `APPLE_ID`、`APPLE_TEAM_ID`、`APPLE_APP_SPECIFIC_PASSWORD` | Apple 公证账号、团队 ID 和专用密码 |
| Secret `SPARKLE_PRIVATE_KEY` | Sparkle 导出的更新私钥 |
| Variable `SPARKLE_PUBLIC_KEY` | Sparkle 更新公钥 |
| Variable `MALEMA_LEADERBOARD_ENDPOINT` | 可选榜单地址，例如 `https://malema.app`。未设置时链接本机服务 |

签名和公证的五个 Secret 沿用 Stela 的命名，在 Malema 仓库的 Actions 设置中配置。
更新密钥用 Sparkle 官方发行包的 `bin/generate_keys --account malema` 生成，
用 `bin/generate_keys --account malema -x <私钥文件>` 导出到受保护的本机目录。
公钥和私钥分别写入上表的配置项。

发布同时提供 `malema-macos-arm64.zip` 和 `appcast.xml`。应用通过 GitHub
的最新 Release 自动检查更新，也可以从应用菜单手动检查。安装新版前会显示确认。
首次从旧内测版迁入时，请手动安装带更新器的签名版本。

## 判定词表

`src/detect.rs` 使用现有定向句式、LDNOOBW 与 dsojevic 底表、Luna 补充词。Luna 标为人身辱骂或产物辱骂的候选按类别进入相应句式，原有粗口用法规则继续生效。只有满足对应目标句式时才计入。

判定会排除引文、代码、普通感叹和「垃圾回收」这类固定技术用语。报表采用精准率优先口径。对于没有可靠模型记录的会话，模型归属会留空。

公开底表、Luna 补充词表和本机复核结果收录在 [候选词表说明](lexicon/README.md)。

## AI 复核

先用工具和日期筛选，再点侧栏或右上角的「AI 复核」。只复核规则命中可检查误报。重扫筛选范围内的全部真人消息可以找遗漏。复核入口在 macOS 桌面应用里。

- 本地 Agent 支持 Codex 和 Claude Code，使用本机已登录的 CLI。Codex 使用 GPT‑5.6‑Luna low，Claude Code 使用 CLI 的默认模型。
- Jev 使用 TypeSafe 官方的 System One 接口和 `jev-latest`。在 [TypeSafe 控制台](https://console.typesafe.ai/keys) 创建 API Key，填入复核窗口，或在启动应用前设置 `TYPESAFE_API_KEY`。密钥仅用于本次操作。[TypeSafe 官方接入文档](https://docs.typesafe.ai/introduction/quickstart)
- 自定义接口有两个判定工具可选。选择「自定义 · Jev System One」时填完整接口地址，例如 `https://example.com/v1/systemone`。选择「自定义 · OpenAI 兼容」时填 Chat Completions 地址，以 `/v1` 结尾的地址会自动补全 `/chat/completions`。两项均需填写模型名，API Key 可选。

模型会在点击「开始复核」后收到脱敏开头或结尾片段。规则命中时优先截取证据附近的内容。本地 Agent 按顺序返回单字符判定，Jev 按消息编号返回结构化 Choice 答案。AI 复核窗口显示本次确认和新发现的记录。审查标准见 [复核 Prompt](src/review-prompt.txt)。
