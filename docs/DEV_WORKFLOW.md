# 开发与发布流程

## 环境边界

| 环境 | 运行内容 | 密钥来源 |
|---|---|---|
| 本地 WSL Docker | PostgreSQL、memoir-api | `.env.development` |
| 微信开发者工具 | 小程序 | 只有 `API_BASE_URL` |
| GitHub Actions | 测试、构建、部署 | GitHub Secrets |
| 生产服务器 | PostgreSQL、memoir-api | Actions 生成的 `/opt/memoir/deploy/.env` |

`LOCAL_AGENT_BASE_URL` 和 `LOCAL_AGENT_MODEL` 是本地 AI 配置，不上传到
GitHub。生产环境使用 `LLM_API_BASE`、`LLM_API_KEY` 和 `LLM_MODEL`。

## 首次本地启动

```bash
cd /mnt/g/project/memoir
cp .env.development.example .env.development
chmod +x scripts/*.sh
./scripts/local_dev.sh start
```

如果已有 `.env.development`，不要覆盖。

验证：

```bash
curl -fsS http://127.0.0.1:18081/health
docker compose -f docker-compose.dev.yml --env-file .env.development ps
```

停止不会删除 PostgreSQL 数据卷：

```bash
./scripts/local_dev.sh stop
```

## 小程序

开发者工具 / 同 Wi‑Fi 真机调试本地 API：

```bash
# WSL
./scripts/local_dev.sh start          # 会调用 miniprogram-env.sh local
# 或单独：
./scripts/miniprogram-env.sh local
```

`local` 会尽量写入 **Windows 局域网 IP**（如 `192.168.x.x:18081`），**不要**用
WSL 虚拟网段 `172.26.x`——手机访问不到。

手机真机还要在 Windows 上做端口转发（管理员 PowerShell）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\win_expose_api.ps1
```

然后：手机与电脑同一 Wi‑Fi → 开发者工具重新编译 → **真机调试**（比「预览」更稳）。

体验版 / 外网好友应使用远程 API：

```bash
./scripts/miniprogram-env.sh remote
```

`127.0.0.1` 在手机上指手机自己；远程正式环境最终需要 **HTTPS + 合法域名**。

## 发布

```text
open a pull request
  -> GitHub Actions test
manual workflow_dispatch (aliyun or wsl)
  -> GitHub Actions test
  -> build immutable image
  -> push GHCR + Aliyun ACR
  -> Self-hosted Runner prefers GHCR and falls back to ACR
  -> docker compose pull/up
  -> /health
```

镜像标签使用完整 Git commit SHA，便于定位和回滚。

部署 job 运行在组织级 `[self-hosted, aliyun]` Runner 上。Actions 需要以下
GitHub Secrets：

- `MEMOIR_PG_PASSWORD`
- `JWT_SECRET`
- `LLM_API_BASE`
- `LLM_API_KEY`
- `LLM_MODEL`
- `WECHAT_APP_ID`
- `WECHAT_APP_SECRET`
- `ALIYUN_ACR_PASSWORD`

ACR 使用仓库变量 `ALIYUN_ACR_REGISTRY`、`ALIYUN_ACR_USERNAME` 和
`MEMOIR_ACR_IMAGE_NAME`。首次部署前还需要将仓库变量
`PRODUCTION_DEPLOY_ENABLED` 设置为 `true`。push 不触发此 workflow；生产构建和部署
必须手动运行。变量未启用时，手动运行只测试和构建镜像，不连接目标服务器。

生产数据库密码首次初始化后不能只靠修改环境变量完成轮换；轮换时还需要在
PostgreSQL 中执行对应的密码修改。
