# Docker 与 Jenkins 部署

当前容器部署包含一个服务：

- `web-api`：Rust HTTP API，容器端口 `10001`。

MongoDB 不在 Compose 中；配置中的 MongoDB 地址必须能从容器网络访问。
该 MongoDB 必须是副本集或支持事务的分片集群；Web API 启动时会执行 `hello`
能力校验，standalone 会直接失败。
首次启用唯一索引前必须按 `docs/mongodb-indexes.md` 查重；索引冲突会阻止
Web API 启动。

## 文件职责

- `Dockerfile`：构建后端镜像，只包含 `web-api`。
- `docker-compose.yml`：本地构建和容器联调。
- `docker-compose.production.yml`：生产部署，只接受 digest 固定的镜像。
- `Jenkinsfile`：选择 Git ref、构建推送镜像、归档发布清单并 SSH
  部署。
- `scripts/release-image.sh`：构建、推送镜像并生成 digest 发布清单。
- `scripts/release-deploy.sh`：部署或回滚发布清单。

## 本地 Compose

先准备真实配置：

```bash
cp config.toml.example config.toml
```

示例中的 JWT secret 会被启动校验故意拒绝；必须改为至少 32 个随机字节。真实
`config.toml` 不得提交到仓库。

`config.toml` 中的 `127.0.0.1` 指向容器自身，不能用于连接宿主机 MongoDB。
请使用容器可路由的内网地址；Docker Desktop 可按环境使用
`host.docker.internal`。

`manage.sh` 会把本地后端容器 UID/GID 映射为当前用户。日志默认输出到容器
stdout/stderr 并由 Docker 限额轮转。直接执行 `docker compose` 时，可通过
`RS_PROJECT_TEMPLATE_LOCAL_UID` 和 `RS_PROJECT_TEMPLATE_LOCAL_GID` 显式指定。

验证：

```bash
docker compose config --quiet
./manage.sh status
./manage.sh health
```

## Jenkins 参数

Jenkins Agent 需要 Docker BuildKit、Docker Compose v2、Git、SSH 和 SCP。
Pipeline 还依赖 Git Parameter、SSH Agent 插件，以及 ID 为 `deploy-ssh`
的 SSH 凭据。

关键参数：

- `GIT_REF`：主项目 branch、tag 或 commit。
- `REGISTRY_HOST`：Jenkins Agent 和部署机都能访问的镜像仓库。
- `DEPLOY_TARGET`：`user@host` 形式的 SSH 目标。
- `DEPLOY_DIR`：部署机上的绝对目录。
- `RS_PROJECT_TEMPLATE_API_HOST_PORT`：Web API 宿主机端口。

Pipeline 使用 Git SHA 与 Jenkins build number 标记镜像。推送后读取镜像仓库
返回的 RepoDigest，生成 `release-artifacts/<tag>.env`。生产 Compose 不包含
`build:`，部署机只运行清单中的 `repository@sha256:...`。

## 生产主机准备

部署用户默认为 `root`，因为部署脚本需要把敏感配置收敛到确定权限。
Jenkins 不传输 `config.toml`；首次部署前必须单独准备：

```bash
install -d -m 0750 /opt/rs-project-template
install -m 0640 /secure/path/config.toml \
  /opt/rs-project-template/config.toml
```

部署脚本会确保：

- `config.toml` 为 `root:root 0640`。
- 后端以非 root 用户运行，只附加 root group 读取只读配置挂载。
- 容器启用 `no-new-privileges`、移除 Linux capabilities。
- Docker `json-file` 日志限制为 `10m`、保留 `3` 个文件。

本次后台 JWT 增加账号版本校验；从旧版本升级后，已有后台 token 会失效，管理员需要重新
登录。部署后操作人员需要重新登录。

## 健康检查与回滚

部署完成必须通过：

- Web API `GET /health`。

新版本失败且存在上一份发布清单时，部署脚本会重新拉起上一版本并复检。手工回滚：

```bash
cd /opt/rs-project-template
scripts/release-deploy.sh current
scripts/release-deploy.sh rollback <release-tag>
```
