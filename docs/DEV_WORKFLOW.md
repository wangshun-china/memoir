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

开发者工具连接本地 API：

```bash
./scripts/miniprogram-env.sh local
```

真机、体验版和正式版连接远程 API：

```bash
./scripts/miniprogram-env.sh remote
```

真机中的 `127.0.0.1` 指手机本身，因此真机必须使用远程 HTTPS 域名。
在 WSL NAT 网络下，`local` 脚本会自动把当前 WSL IP 写入小程序配置，
避免依赖 Windows 的 localhost 转发。

## 发布

```text
git push main
  -> GitHub Actions test
  -> build ghcr.io/<owner>/memoir-api:<commit-sha>
  -> push image
  -> SSH 120.26.186.0
  -> docker compose pull/up
  -> /health
```

镜像标签使用完整 Git commit SHA，便于定位和回滚。

Actions 需要以下 GitHub Secrets：

- `PROD_SSH_HOST`
- `PROD_SSH_USER`
- `PROD_SSH_PRIVATE_KEY`
- `PROD_SSH_KNOWN_HOSTS`
- `MEMOIR_PG_PASSWORD`
- `JWT_SECRET`
- `LLM_API_BASE`
- `LLM_API_KEY`
- `LLM_MODEL`
- `WECHAT_APP_ID`
- `WECHAT_APP_SECRET`

首次部署前还需要将仓库变量 `PRODUCTION_DEPLOY_ENABLED` 设置为 `true`。
变量未启用时，Actions 只测试和构建镜像，不连接生产服务器。

生产数据库密码首次初始化后不能只靠修改环境变量完成轮换；轮换时还需要在
PostgreSQL 中执行对应的密码修改。
