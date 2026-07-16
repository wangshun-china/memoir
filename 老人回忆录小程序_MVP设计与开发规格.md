# 老人回忆录小程序：MVP 产品与技术设计规格

> 面向本地开发 Agent 的实施文档  
> 状态：已确认的第一阶段方案  
> 日期：2026-07-15

## 0. 给开发 Agent 的直接指令

请基于本文档实现一个“通过 AI 文字采访，将老人零散人生经历整理为故事，并进一步生成回忆录章节”的微信小程序 MVP。

实施时遵循以下原则：

1. 第一阶段只做文字，不做录音、语音识别、照片、视频、OCR、数字人和本地模型推理。
2. 用户可自行使用豆包输入法、微信输入法等完成语音转文字，本系统只接收文字。
3. 后端使用 Rust 单体服务，AI 能力通过第三方大模型 API 调用。
4. 核心链路必须先跑通：采访 → 故事卡片 → 自动归类 → 章节生成 → 用户确认。
5. 不要提前引入微服务、Redis、消息队列、向量数据库、知识图谱数据库、Kubernetes 或复杂 Agent 框架。
6. AI 可以润色表达，但不得虚构事实。所有生成内容必须可追溯到原始采访材料。
7. 产品的关键指标不是“功能数量”，而是用户是否认为：“这确实是我的故事，而不是 AI 编的故事。”

---

## 1. 产品定义

### 1.1 一句话定位

一个通过 AI 文字采访，帮助老人及其子女把零散人生经历整理成完整回忆录的微信小程序。

### 1.2 第一性原理

用户真正需要的不是一个普通聊天机器人，而是一套“人生材料生产系统”：

```text
零散讲述
→ 追问和补充细节
→ 结构化故事卡片
→ 多个相关故事自动归入章节
→ 章节草稿
→ 用户补充、修改、确认
→ 完整回忆录
```

### 1.3 目标用户

主要存在两类用户：

- 内容讲述者：老人本人。
- 创建者和协助者：子女或其他家庭成员。

MVP 可以先不区分复杂角色权限，但数据模型应保留“创建者”和“回忆录主人”两个概念。

### 1.4 核心价值

- 老人不知道从哪里讲起：采访者主动提出具体、低压力的问题。
- 老人讲述零散、跳跃、重复：系统自动整理为故事单元。
- 老人缺乏写作能力：写作者将已确认素材整理为章节。
- 子女无暇长期采访和编辑：系统承担大部分整理工作。

---

## 2. MVP 范围

### 2.1 必须实现

1. 微信登录。
2. 创建一本回忆录。
3. 选择采访主题并开始文字采访。
4. 采访者根据上下文逐轮追问。
5. 将一段采访自动整理为一个或多个故事卡片。
6. 提取故事的时间、地点、人物、主题、情感、人生阶段等元数据。
7. 自动推荐故事的主要章节和相关章节。
8. 用户确认或修改归类结果。
9. 根据多个故事、背景事实和感悟生成章节草稿。
10. 支持章节编辑、重新生成、补充采访和确认。
11. 保存章节版本以及所使用的故事/事实来源。

### 2.2 明确不做

- 自建语音识别。
- 保存录音、照片和视频。
- OCR、老照片修复、数字人。
- 本地部署大模型。
- 实体书排版和印刷下单。
- 复杂多人协作和精细权限体系。
- 知识图谱数据库。
- 独立向量数据库。
- 完全自治的多 Agent 系统。
- App 和 H5 正式客户端。

### 2.3 后续可能增加，但当前只留扩展点

- 照片、录音与证据材料。
- PDF/Word/实体书导出。
- 家庭成员协作。
- 史实查询与时间冲突检查。
- 多种写作风格。
- 长期记忆检索和向量搜索。

---

## 3. 产品核心对象

系统至少包含以下层级：

```text
Memoir（一本回忆录）
├── InterviewSession（一次采访会话）
│   └── InterviewMessage（原始问答消息）
├── Story（故事卡片）
│   ├── Fact（背景事实）
│   └── Reflection（感悟，可先作为故事字段或标签）
├── Chapter（章节）
│   └── ChapterVersion（章节版本）
└── StoryChapterRelation（故事与章节关系）
```

### 3.1 故事卡片是核心中间产物

采访不能直接生产整本回忆录。系统应先把讲述整理成可独立使用的“故事卡片”。

示例：

```json
{
  "title": "雪天上学",
  "summary": "小学时冒雪步行上学，鞋子湿透后老师让他在炉边烤鞋。",
  "life_stage": "童年",
  "time_text": "小学时期",
  "location_text": "老家到乡村学校",
  "people": ["本人", "老师"],
  "themes": ["求学", "家庭贫困", "师生情"],
  "emotion": ["艰苦", "温暖"],
  "cause": "大雪天仍需步行去学校",
  "process": "鞋子被雪水浸湿",
  "result": "老师让他在炉子旁烤鞋",
  "missing_details": ["学校距离", "老师姓名"],
  "source_message_ids": [101, 103, 105],
  "status": "draft"
}
```

故事卡片必须保留原始消息来源，以便生成章节时进行事实追溯。

### 3.2 事实、故事和感悟的区别

- 事实：出生年份、学校、单位、家庭成员等可陈述信息。
- 故事：包含人物、场景、事件发展或变化的叙事单元。
- 感悟：用户对家庭、时代、人生选择的评价和回望。

一个章节通常由“背景事实 + 若干具体故事 + 回望感悟”构成。

---

## 4. 两个核心 Skill

MVP 使用两套明确工作流，而不是复杂自治 Agent。

## 4.1 采访者 Skill

### 目标

持续挖掘真实、具体、可写作的素材，同时避免重复提问和造成心理压力。

### 输入上下文

```text
回忆录主人基本信息
当前采访主题
当前主题摘要
最近若干轮对话
相关已知事实和故事
已经问过的问题
尚缺失的材料
用户的跳过/隐私偏好
```

### 追问策略

采访者应优先选择以下类型之一：

- 时间：哪一年、当时多大、之前和之后发生了什么。
- 场景：在哪里、周围是什么样、天气或环境如何（只能询问，不能擅自补充）。
- 人物：当时还有谁、彼此关系、人物性格。
- 行动：具体做了什么、事情如何发展。
- 因果：为什么这样做、产生了什么影响。
- 感受：当时怎么想、现在如何看待。
- 感官细节：声音、味道、衣着、物件、食物。
- 冲突变化：困难、意外、遗憾、转折。

不应频繁使用“请详细说说”这类空泛问题。

### 交互规则

- 一次只问一个主要问题。
- 问题应短、具体、容易回答。
- 用户可选择“不知道怎么回答”“换一个问题”“不想说”“结束本次采访”。
- 对敏感经历不连续逼问。
- 当素材已形成完整故事，主动建议结束本轮或转入下一个故事。

### 建议结构化输出

```json
{
  "reply": "当时家里最值钱的一件东西是什么？",
  "question_type": "object_detail",
  "covered_points": ["家庭经济困难"],
  "missing_points": ["生活细节", "具体事件", "家庭成员反应"],
  "potential_story": true,
  "should_end_session": false,
  "reason": "需要通过具体物件将抽象的‘贫穷’转化为可写作细节"
}
```

## 4.2 写作者 Skill

### 目标

把已确认的故事、事实和感悟编排成连贯章节，保持第一人称和用户自身语言气质。

### 强制事实约束

```text
所有事实必须来自提供的原始材料、故事卡片或已确认事实。
禁止自行补充天气、外貌、动作、对话、心理活动、历史背景和因果关系。
可以调整语序、衔接段落和润色表达，但不得改变事实含义。
无法确认的内容应省略，或列入待补充问题。
```

### 第一版写作方式

先支持以下三种，不直接宣称模仿具体作家：

1. 朴素纪实：第一人称、时间顺序、短句、少修辞、保留口语感。
2. 温情家庭：突出亲情、生活细节和情绪变化，但保持克制。
3. 时代故事：适当呈现个人经历与时代环境的关系，背景事实必须有材料依据。

默认使用“朴素纪实”。

### 建议结构化输出

```json
{
  "title": "雪天里的那间教室",
  "content": "……",
  "used_story_ids": [12, 18, 21],
  "used_fact_ids": [31, 32],
  "uncertain_claims": [],
  "missing_materials": ["老师姓名尚未确认", "学校距离不明确"],
  "follow_up_questions": ["那位老师叫什么名字？", "你每天上学大概要走多久？"]
}
```

---

## 5. 自动归类与章节生成

### 5.1 是否自动归类

是。每个故事完成后，系统自动推荐主要章节和相关章节，但用户拥有最终确认权。

### 5.2 多对多关系

同一个故事可能同时涉及“求学经历”“父亲”“第一次离家”“人生转折”等主题。因此：

- 每个故事只有一个 `primary_chapter`。
- 一个故事可以关联多个 `related_chapters`。
- 一个章节包含多个故事。

建议关系字段：

```text
story_id
chapter_id
relevance_score
relation_type: primary | related
confirmed_by_user
classification_reason
```

### 5.3 归类过程

```text
故事卡片生成
→ 提取人生阶段、时间、主题、人物、地点
→ 与现有章节规则匹配
→ 大模型进行语义排序
→ 返回主要章节和候选章节
→ 用户确认或调整
```

第一版不需要向量聚类。可以使用规则筛选 + 大模型 JSON 分类。

### 5.4 默认章节模板

创建回忆录时预置：

1. 童年与家庭
2. 求学经历
3. 青年时代
4. 工作与事业
5. 婚姻与家庭
6. 人生转折
7. 子女与家庭生活
8. 退休与晚年
9. 我想留下的话

章节不要求按顺序完成。用户可修改名称、删除或新增章节。

当某个主题积累了多个故事时，AI 可以建议拆分章节，例如：

> 已记录 6 个关于“下乡经历”的故事，建议单独创建《在北大荒的那些年》章节。

必须由用户确认后再创建。

### 5.5 章节生成时机

不要等所有人生故事采集完再写。推荐：

```text
积累 3—5 个相关故事
→ 生成章节初稿
→ 检测材料缺口
→ 补充采访
→ 更新章节版本
```

用户应尽早看到成果，以增强继续讲述的动力。

---

## 6. 用户流程

### 6.1 创建回忆录

收集最少信息：

- 回忆录主人姓名。
- 出生年份（可选）。
- 出生地（可选）。
- 希望系统如何称呼。
- 创建者与主人关系（可选）。

### 6.2 首页/人生目录

展示：

- 回忆录名称。
- 已完成故事数量。
- 已完成/进行中章节。
- 推荐继续采访的主题。
- 最近生成的章节。

不要把“人生完成度百分比”作为强约束，因为人生材料很难客观量化。可显示“已记录 12 个故事、已完成 3 章”。

### 6.3 采访页面

界面应明确显示：

```text
正在整理：童年与家庭
本次采访：小时候住过的家
```

交互区域：

- AI 问题。
- 大字号文本输入框。
- 提示用户可使用系统语音输入法。
- 发送按钮。
- 不知道怎么回答。
- 换一个问题。
- 这个问题不想说。
- 结束本次采访。

### 6.4 故事卡片确认页

采访结束后展示：

- 故事标题。
- 故事摘要。
- 时间、地点、人物、主题。
- 主要章节和相关章节。
- 缺失信息。

操作：确认、修改、补充采访、删除。

### 6.5 章节页

显示章节草稿，并提供：

- 编辑。
- 补充采访。
- 重新生成。
- 查看使用了哪些故事。
- 确认本章。
- 查看历史版本。

---

## 7. 技术架构

### 7.1 总体结构

```text
微信小程序前端
    │ HTTPS / JSON
    ▼
Caddy 或 Nginx（443）
    ▼
Rust Axum 单体后端（容器内 8080）
    ├── PostgreSQL
    └── 第三方大模型 API
```

### 7.2 前端发布方式

小程序前端不能像 H5 一样作为长期运行的 Docker 容器部署。

```text
小程序源码
→ 微信开发者工具或 miniprogram-ci 构建上传
→ 微信平台审核和发布
→ 微信客户端分发运行
```

Docker 只负责后端运行环境。后续可以用一次性 CI 容器构建和上传小程序，但不属于生产常驻服务。

### 7.3 前后端通信

正式环境使用 HTTPS 域名：

```text
https://api.example.com
→ DNS 解析到服务器公网 IP
→ Caddy/Nginx
→ Rust 后端
```

小程序通过 `wx.request` 调用 API。微信公众平台需将该 HTTPS 域名配置为 `request` 合法域名。

禁止把以下信息写入小程序源码：

- 大模型 API Key。
- 微信 AppSecret。
- 数据库密码。
- JWT 签名密钥。

### 7.4 推荐技术栈

```text
后端框架：Axum
异步运行时：Tokio
序列化：Serde
数据库：PostgreSQL
数据库访问：SQLx
HTTP 客户端：Reqwest
日志：Tracing
鉴权：JWT 或服务端会话 Token
反向代理：Caddy（优先，证书自动化更简单）或 Nginx
部署：Docker Compose
```

### 7.5 初期不需要的组件

```text
Redis
Kafka / RabbitMQ
Qdrant / Elasticsearch
Kubernetes
独立 AI Worker
复杂多 Agent 框架
```

---

## 8. 数据库设计建议

以下为逻辑结构，具体类型由实现决定。

### users

```text
id
wechat_openid
nickname
created_at
updated_at
```

### memoirs

```text
id
owner_user_id（可空，老人本人未登录时）
creator_user_id
title
subject_name
birth_year
birth_place
preferred_name
status
created_at
updated_at
```

### chapters

```text
id
memoir_id
title
sort_order
status: empty | collecting | draft | confirmed
summary
created_at
updated_at
```

### interview_sessions

```text
id
memoir_id
chapter_id（可空）
topic
status: active | finished | cancelled
summary
started_at
finished_at
```

### interview_messages

```text
id
session_id
role: user | assistant | system
content
question_type（assistant 时可用）
created_at
```

### stories

```text
id
memoir_id
title
summary
life_stage
time_text
location_text
people_json
themes_json
emotion_json
cause_text
process_text
result_text
missing_details_json
status: draft | confirmed | archived
created_at
updated_at
```

### story_sources

```text
story_id
message_id
```

### story_chapter_relations

```text
story_id
chapter_id
relevance_score
relation_type: primary | related
confirmed_by_user
classification_reason
```

### facts

```text
id
memoir_id
category
content
time_text
location_text
people_json
confidence
source_message_id
confirmed
created_at
```

### chapter_versions

```text
id
chapter_id
version_number
title
content
style
used_story_ids_json
used_fact_ids_json
missing_materials_json
created_by: ai | user
created_at
```

正式实现可将部分 JSON 字段使用 PostgreSQL JSONB。

---

## 9. 建议 API

### 鉴权

```text
POST /api/v1/auth/wechat
POST /api/v1/auth/refresh
```

### 回忆录

```text
POST /api/v1/memoirs
GET  /api/v1/memoirs
GET  /api/v1/memoirs/{memoir_id}
PATCH /api/v1/memoirs/{memoir_id}
```

### 章节

```text
GET  /api/v1/memoirs/{memoir_id}/chapters
POST /api/v1/memoirs/{memoir_id}/chapters
PATCH /api/v1/chapters/{chapter_id}
POST /api/v1/chapters/{chapter_id}/generate
POST /api/v1/chapters/{chapter_id}/confirm
GET  /api/v1/chapters/{chapter_id}/versions
```

### 采访

```text
POST /api/v1/memoirs/{memoir_id}/interviews
GET  /api/v1/interviews/{session_id}
POST /api/v1/interviews/{session_id}/messages
POST /api/v1/interviews/{session_id}/finish
```

### 故事

```text
GET   /api/v1/memoirs/{memoir_id}/stories
GET   /api/v1/stories/{story_id}
PATCH /api/v1/stories/{story_id}
POST  /api/v1/stories/{story_id}/confirm
POST  /api/v1/stories/{story_id}/classify
DELETE /api/v1/stories/{story_id}
```

### 推荐消息提交事务流程

```text
1. 保存用户原始回答，释放数据库连接。
2. 调用采访者 Skill。
3. 保存 AI 的下一问题和结构化元数据。
4. 如检测到可成形故事，更新故事草稿。
5. 返回下一问题、当前故事进度和可选操作。
```

注意：等待大模型 API 时不要长期占用 PostgreSQL 连接。

---

## 10. 大模型调用设计

### 10.1 不要每轮发送全部历史

采访者上下文只发送：

```text
当前采访主题
当前主题摘要
最近 6—12 条消息
与主题相关的事实和故事摘要
已经问过的问题
尚缺失信息
```

当会话变长时，定期生成可覆盖旧消息的会话摘要，但原始消息仍保存在数据库。

### 10.2 输出必须是可校验 JSON

- 使用严格 JSON Schema。
- 后端解析失败时允许一次修复重试。
- 不要直接相信模型返回的数据库 ID。
- 所有 `story_id/chapter_id` 必须由后端从允许集合中校验。

### 10.3 模型并发

初期使用 `tokio::sync::Semaphore` 限制大模型并发，例如 5 个。

不需要消息队列。接口超时可设置为 60—120 秒；章节生成可后续增加流式响应。

### 10.4 Prompt 版本管理

采访者和写作者 Prompt 应带版本号：

```text
interviewer_v1
writer_plain_v1
story_extractor_v1
chapter_classifier_v1
```

每次模型调用保存：

```text
prompt_version
model_name
input_token_estimate
output_token_estimate
latency_ms
success/error
```

敏感原文日志默认不输出到普通应用日志。

---

## 11. Rust 项目结构建议

```text
memoir-project/
├── miniprogram/                 # 微信小程序源码
│   ├── pages/
│   ├── components/
│   ├── services/
│   └── app.ts
│
├── server/
│   ├── Cargo.toml
│   ├── Dockerfile
│   ├── migrations/
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── error.rs
│       ├── state.rs
│       ├── auth/
│       ├── memoirs/
│       ├── interviews/
│       ├── stories/
│       ├── chapters/
│       ├── llm/
│       │   ├── client.rs
│       │   ├── interviewer.rs
│       │   ├── writer.rs
│       │   ├── extractor.rs
│       │   └── classifier.rs
│       └── db/
│
├── deploy/
│   ├── docker-compose.yml
│   ├── Caddyfile
│   └── .env.example
│
└── docs/
    └── product-spec.md
```

后端保持模块化单体，不拆微服务。

---

## 12. 部署与资源

### 12.1 Docker Compose

生产服务器常驻容器：

```text
memoir-api    Rust Axum
postgres      PostgreSQL
caddy         HTTPS 和反向代理
```

小程序前端不属于 Compose 常驻服务。

### 12.2 推荐服务器资源

当前文字型 MVP：

```text
推荐：2 核 CPU / 4 GB 内存 / 40 GB 以上 SSD / 3 Mbps 以上带宽
最低：1 核 CPU / 2 GB 内存 / 30 GB SSD
```

因为模型由第三方 API 运行，本地 CPU 压力很小。主要内存由 Linux、Docker 和 PostgreSQL 使用。

### 12.3 网络暴露

公网仅开放：

```text
80（跳转 HTTPS 和证书申请）
443（正式 API）
```

Rust 8080 和 PostgreSQL 5432 只在 Docker 内部网络暴露，不直接开放公网。

### 12.4 备份

即使只有文字，人生材料也不可替代。至少实现：

- 每日 PostgreSQL 自动备份。
- 保留最近 7—14 份。
- 定期复制到服务器之外的位置。
- 日志轮转，防止 Docker 日志写满磁盘。

---

## 13. 安全与隐私

1. 回忆录默认私有，禁止公开可枚举 URL。
2. 所有资源查询必须校验当前用户是否拥有权限。
3. 使用随机 UUID 或不可预测 ID。
4. API Key 和密钥只通过环境变量注入。
5. 大模型服务商可能接触用户文本，应在隐私政策中明确说明。
6. 不将完整采访原文写入普通日志。
7. 对章节删除、回忆录删除等操作做二次确认或软删除。
8. 限制单条输入长度和接口频率，防止滥用模型额度。
9. 将“这个问题不想说”记录为采访偏好，避免后续重复追问同类敏感内容。

---

## 14. MVP 验收标准

找 5—10 名真实用户，每人完成至少一次 20—30 分钟的文字采访。重点验证：

1. 用户能否在没有培训的情况下创建回忆录并开始采访。
2. 用户是否愿意连续回答至少 8—10 个问题。
3. 采访者是否能够提出具体追问，而非机械重复。
4. 系统能否形成至少一个结构完整的故事卡片。
5. 自动归类是否大体正确，用户是否能轻松调整。
6. 3—5 个故事能否生成一章连贯草稿。
7. 生成内容是否存在虚构的天气、对话、心理或细节。
8. 用户是否认为文风“像自己说的话”。
9. 用户是否愿意继续完成下一次采访。
10. 子女是否认为成稿值得保存、修改或分享。

核心质性标准：

> “这确实是我的故事，而不是 AI 编的故事。”

---

## 15. 建议开发顺序

### 阶段 1：最小闭环

- Rust 项目初始化、PostgreSQL、Docker Compose。
- 微信登录或先用开发环境模拟用户。
- 创建回忆录和默认章节。
- 文字采访页面。
- 采访者 Skill 调用。
- 保存问答记录。

### 阶段 2：故事化

- 故事抽取 JSON。
- 故事卡片编辑和确认。
- 故事来源追溯。
- 自动章节分类和用户调整。

### 阶段 3：章节化

- 章节生成。
- 使用故事/事实来源记录。
- 补充采访建议。
- 章节版本管理、编辑和确认。

### 阶段 4：真实用户验证

- 邀请 5—10 名用户。
- 记录失败案例：重复提问、虚构、归类错误、用户中途退出。
- 基于数据迭代 Prompt 和交互，而不是扩充外围功能。

---

## 16. 当前仍可后续决定的问题

这些问题不阻碍 MVP 开发，可在实现中使用合理默认值：

1. 小程序采用原生框架还是 Taro。默认建议原生 TypeScript，减少跨端抽象。
2. 大模型供应商。应通过统一 `LlmClient` 接口隔离，支持替换 OpenAI 兼容 API。
3. 是否第一版就实现微信正式登录。开发期可先使用固定测试用户，但上线前必须实现。
4. 章节生成是同步还是异步。MVP 可同步；超过超时后再引入任务状态。
5. 事实是否必须逐条让用户确认。第一版可只在故事卡片确认时整体确认，并保留单独编辑能力。

---

## 17. 实施禁区

开发 Agent 不应在没有明确需求时：

- 自行增加照片、录音、语音识别或 OCR。
- 自行引入知识图谱、向量数据库或 RAG。
- 把采访者、写作者拆成多个自治 Agent 循环。
- 为“未来扩展”设计复杂微服务和事件总线。
- 用 AI 自动发布用户回忆录或默认公开内容。
- 允许写作者为了文学效果创造不存在的细节。
- 将小程序前端作为 Nginx 常驻容器部署。

优先交付可运行、可测试、可验证的核心闭环。
