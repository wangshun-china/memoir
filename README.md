# memoir

老人回忆录微信小程序和 Rust Axum 后端。

## 本地开发

数据库和后端都运行在 WSL Docker 中，Windows 主机不要安装 PostgreSQL。

```bash
cd /mnt/g/project/memoir
chmod +x scripts/*.sh deploy/deploy.sh
./scripts/local_dev.sh start
```

服务地址：

- WSL 内 API 健康检查：`http://127.0.0.1:18081/health`
- **管理台**：`http://127.0.0.1:18081/admin/`
- Windows/小程序 API：脚本自动使用当前 WSL IP 和端口 `18081`
- PostgreSQL：`127.0.0.1:5433`

管理台：
- **首次访问**会要求创建真实管理员账号（用户名+密码，Argon2 存库），无默认/mock 密码
- 之后用该账号登录；支持在「账号安全」中改密
- 总览/用户/回忆录/用量均为数据库真实数据

常用命令：

```bash
./scripts/local_dev.sh start
./scripts/local_dev.sh stop
./scripts/local_dev.sh rebuild
./scripts/local_dev.sh logs
./scripts/local_dev.sh status
```

`start` 始终执行 `docker compose up -d --build`。没有代码变化时 Docker
复用缓存；代码变化时自动重建 API 镜像。

本地脚本会读取 Windows 机器级的 `LOCAL_AGENT_BASE_URL` 和
`LOCAL_AGENT_MODEL`，并分别映射为后端的 `LLM_API_BASE` 和 `LLM_MODEL`。
API Key 仍来自 gitignored 的 `.env.development`。

## 小程序 API 环境

```bash
./scripts/miniprogram-env.sh local
./scripts/miniprogram-env.sh remote
```

- `local`：自动生成 `http://<WSL_IP>:18081/api/v1`
- `remote`：`http://api.wangshun.work/api/v1`（Stage 1，后续切换 HTTPS）

生成的 `miniprogram/config/env.ts` 不提交。小程序中不得放入 AppSecret、
LLM Key、JWT Secret 或数据库密码。

## CI/CD

`.github/workflows/pipeline.yml`：

1. PR 和 `main` push 执行格式检查、Clippy 和测试。
2. `main` push 构建不可变 GHCR 镜像。
3. GitHub Actions 通过 SSH 将运行时密钥写入服务器。
4. 服务器拉取镜像并运行 Docker Compose。
5. 验证容器健康检查和公网 HTTPS 健康检查。

生产服务器只拉取镜像，不在服务器编译源码。生产运行文件位于
`/opt/memoir/deploy`。

详细说明见 [docs/DEV_WORKFLOW.md](docs/DEV_WORKFLOW.md) 和
[deploy/README.md](deploy/README.md)。
