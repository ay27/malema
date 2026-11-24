# 骂了吗全球被骂榜

TypeScript Worker 接收 Rust 桌面端主动开启后的统计，D1 保存周累计和公开榜单快照。公开网页在 `public/`，客户端和 Worker 共用上级目录的 `leaderboard-contract.json`。

## 本机内测

需要 Node.js 22 或更新版本。从本目录运行：

```sh
npm ci
npm run migrate:local
npm run dev
```

本地 beta 桌面应用的「全球榜单」会在默认浏览器打开 `http://127.0.0.1:8787`。侧栏底部的「匿名提交」开关默认关闭，开启后首次提交包含最近 7 天的统计。网页开始时为空，达到门槛后才有榜单。

Wrangler 本地预览在有人查看本期榜时，每 10 分钟最多刷新一次快照。也可以手动触发定时任务，用来立即检查新数据：

```sh
curl 'http://127.0.0.1:8787/__scheduled?cron=*/10+*+*+*+*'
```

上报接口 `POST /api/v1/contributions` 接收协议版本、规则版本、周期代号、UTC 周和计数。每周由一个递增版本的完整统计覆盖。榜单接口 `GET /api/v1/leaderboard?period=current` 或 `period=previous` 只返回已达门槛的快照。每个安装每周、每个工具 × 模型 × 语言最多计 5,000 条消息，客户端对同一个样本同时统计分子和分母。合计榜仅汇总通过门槛的语言分组。

## Cloudflare 部署时的配置

本目录的 `wrangler.jsonc` 是本机配置。生产或远程内测应使用单独的 Worker 和独立 D1 库。填入真实数据库 ID，分别应用各自的 migration，在准备发布时绑定相应的 Custom Domain。

- `INGEST_MODE=invite` 允许带邀请凭证的上报。把 `INGEST_INVITE` 设为 Worker secret。内测启动时通过环境变量 `MALEMA_LEADERBOARD_INVITE` 向客户端提供邀请。
- `INGEST_MODE=public` 开启公开写入。未设置有效模式时写入关闭，网页仍读取最后的数据快照。
- 桌面端编译变量 `MALEMA_LEADERBOARD_ENDPOINT` 支持 `https://beta.malema.app` 和 `https://malema.app`，默认连接回环地址。开发和测试环境无需真实会话或 Cloudflare 账户。
