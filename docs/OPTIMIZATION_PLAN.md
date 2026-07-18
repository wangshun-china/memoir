# 优化实施计划（2026-07）

按影响/工作量分阶段，本分支逐步落地。

## 阶段 1 — 可靠性与体验（本次优先）

1. 生成幂等：`generation_status` + claim，防止双点覆盖
2. 索引：会话 resume / 进度查询
3. 聊天响应直接 append + 乐观气泡 + loading 文案
4. `ensureLogin` 短缓存，减少 `/me`
5. 继续采访强制 `sessionId`；创建/开始带 `chapterId`
6. CI 增加 `verify_goal_helpers.mjs`

## 阶段 2 — 性能

1. 满 20 轮**自动生成异步化**（不阻塞聊天返回）
2. 章节列表默认不带全文 `content`（`include_content`）
3. 服务端 LLM 上下文只查最近 N 条消息

## 阶段 3 — 运维（轻量）

1. CORS：管理台同域，默认不再对任意 Origin 完全放开（可 env 覆盖）
2. 生产密钥默认值启动告警（日志）
3. HTTPS / 正式域名：文档提示，需网关侧操作

## 非目标（本轮不做）

- 故事卡片全链路、Redis/队列中间件
- 换 LLM 模型
- 语音输入
