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
./scripts/miniprogram-env.sh local
```

如果已有 `.env.development`，不要覆盖。可按需填写 `REMOTE_SSH_*`（远程开关机用）。

验证：

```bash
curl -fsS http://127.0.0.1:18081/health
./scripts/miniprogram-env.sh status
```

## 环境切换（唯一入口）

```bash
./scripts/miniprogram-env.sh local   # 关远程 → 开本地 → env.ts=local
./scripts/miniprogram-env.sh remote  # 关本地 → 开远程 → env.ts=remote
./scripts/miniprogram-env.sh stop    # 本地+远程全部停
./scripts/miniprogram-env.sh status
```

`local` 会尽量把小程序 API 写成 **Windows 局域网 IP:18081**。真机还需管理员执行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\win_expose_api.ps1
```

手机与电脑同一 Wi‑Fi → 重新编译 → **真机调试**。

远程正式环境最终需要 **HTTPS + 合法域名**。

## 发布

```text
open a pull request
  -> .github/workflows/ci.yml checks
manual .github/workflows/deploy.yml (aliyun or wsl)
  -> build immutable image
  -> push GHCR + Aliyun ACR
  -> Self-hosted Runner prefers GHCR and falls back to ACR
  -> docker compose pull/up
  -> /health
```

镜像标签使用完整 Git commit SHA，便于定位和回滚。

部署 job 运行在所选的组织级 `[self-hosted, aliyun]` 或 `[self-hosted, wsl]`
Runner 上。Actions 需要以下 GitHub Secrets：

- `MEMOIR_PG_PASSWORD`
- `JWT_SECRET`
- `ADMIN_RECOVERY_SECRET`（可选，用于管理员忘记密码）
- `LLM_API_BASE`
- `LLM_API_KEY`
- `LLM_MODEL`
- `WECHAT_APP_ID`
- `WECHAT_APP_SECRET`
- `ALIYUN_ACR_PASSWORD`

ACR 使用仓库变量 `ALIYUN_ACR_REGISTRY`、`ALIYUN_ACR_USERNAME` 和
`MEMOIR_ACR_IMAGE_NAME`。push 不触发部署 workflow；生产构建和部署必须从 GitHub Actions 手动运行
`Build and Deploy`。

生产数据库密码首次初始化后不能只靠修改环境变量完成轮换；轮换时还需要在
PostgreSQL 中执行对应的密码修改。
