# memoir

老人回忆录微信小程序和 Rust Axum 后端。

## 本地开发

数据库和后端都运行在 WSL Docker 中，Windows 主机不要安装 PostgreSQL。

```bash
cd /mnt/g/project/memoir
chmod +x scripts/*.sh deploy/deploy.sh
./scripts/miniprogram-env.sh local
```

统一入口 **`scripts/miniprogram-env.sh`**（`local_dev.sh` 只是薄封装）：

| 命令 | 行为 |
|------|------|
| `local` | **关远程**容器 → **开本地** Docker → 小程序 `env.ts` 指向局域网 API |
| `remote` | **关本地**容器 → **开远程** Docker → 小程序 `env.ts` 指向 `api.wangshun.work` |
| `stop` | **本地 + 远程**容器全部关闭 |
| `status` | 查看两端 compose 状态与当前 `env.ts` |

```bash
./scripts/miniprogram-env.sh local
./scripts/miniprogram-env.sh remote
./scripts/miniprogram-env.sh stop
./scripts/miniprogram-env.sh status
```

本地地址：

- API / 健康检查：`http://127.0.0.1:18081/health`
- 管理台：`http://127.0.0.1:18081/admin/`
- 小程序：优先写入 Windows **局域网 IP**（真机需再跑 `scripts/win_expose_api.ps1`）
- PostgreSQL：`127.0.0.1:5433`

远程 SSH（可选，写在 `.env.development`，勿提交密码）：

```bash
REMOTE_SSH_HOST=120.26.186.0
REMOTE_SSH_USER=root
# REMOTE_SSH_PASS=...   # 有 sshpass 时可用；否则用本机 SSH 密钥
```

本地脚本会读取 Windows 机器级的 `LOCAL_AGENT_BASE_URL` /
`LOCAL_AGENT_MODEL`。API Key 来自 gitignored 的 `.env.development`。

生成的 `miniprogram/config/env.ts` 不提交。小程序中不得放入 AppSecret、
LLM Key、JWT Secret 或数据库密码。

### 微信登录（真实，无 mock）

小程序使用 `wx.login` → `POST /api/v1/auth/wechat`，后端用 `WECHAT_APP_ID` /
`WECHAT_APP_SECRET` 调微信 `jscode2session` 换 **真实 openid**。未配置密钥时接口直接报错，不会伪造 openid。

```bash
# WSL / 服务器环境变量
export WECHAT_APP_ID=你的小程序AppId
export WECHAT_APP_SECRET=你的小程序AppSecret
```

底部 Tab：**首页**（回忆录列表）/ **我的**（资料、登录退出）。

`POST /auth/dev-login` 仅在 `ALLOW_DEV_LOGIN=1` 时开启（CI 自动化测试），小程序不会调用。

## CI/CD

GitHub Actions 分为两个职责明确的 workflow：

1. `.github/workflows/ci.yml` 在 PR 中执行格式检查、Clippy、测试和小程序辅助测试。
2. `.github/workflows/deploy.yml` 仅手动触发，可选择 `aliyun` 或 `wsl`。
3. 部署 workflow 构建不可变镜像并同时推送到 GHCR 和阿里云 ACR。
4. Self-hosted Runner 优先拉取 GHCR，失败时回退 ACR，然后运行 Docker Compose。
5. 最后验证目标环境健康检查；普通 push 不触发构建或部署。

生产服务器只拉取镜像，不在服务器编译源码。生产运行文件位于
`/opt/memoir/deploy`。

详细说明见 [docs/DEV_WORKFLOW.md](docs/DEV_WORKFLOW.md) 和
[deploy/README.md](deploy/README.md)。
