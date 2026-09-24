# Kubernetes 部署

默认部署两个工作负载：

- `erp-api`：后端 API，容器端口 `10001`，配置来自 Secret `erp-api-config` 里的 `config.toml`。
- `erp-client`：管理端，容器端口 `3000`。浏览器里的 API 地址在构建镜像时写入，不在 Pod 环境变量里改。

MongoDB 和 S3 不在默认清单里。MongoDB 必须是副本集；standalone 会在 `erp-api` 启动时被拒绝。S3 参数写在同一份 `config.toml`。开发集群可以另外启用 `deploy/k8s/optional/mongo` 的单节点副本集，那个库没有认证，不能当生产库。

Compose 和 Jenkins 仍按 `backend/DEPLOY.md` 使用，这套清单不替换它们。

## 目录

- `k8s/base`：命名空间 `prod`、两个 Deployment/Service、CLB Ingress。
- `k8s/overlays/local`：镜像标签 `dev`，Ingress 主机 `erp.local` 和 `api.erp.local`。
- `k8s/overlays/production`：前后端各 1 个副本，PodDisruptionBudget 允许维护时驱逐；镜像地址需要改成实际仓库。
- `k8s/examples/web-api-config.example.toml`：配置模板。
- `k8s/optional/mongo`：开发用单节点副本集。
- `erp-client/Dockerfile`：管理端镜像。后端镜像仍用 `backend/Dockerfile`。

入口是腾讯云 TKE 自带的 CLB Ingress（`kubernetes.io/ingress.class: qcloud`），不要安装 ingress-nginx。命名空间是 `prod`。TLS 引用控制台里已有的 Opaque Secret `fsytsl-wsk87cm7`，不在清单里再建证书 Secret。生产 overlay 使用手动创建的共享 CLB `lb-gpk8k2ps`，通过 `enable-group` 和 `existLbId` 注解指定复用，不为 ERP 自动创建独立 CLB。`erp.fushangyunfu.com` 进管理端，`erp-api.fushangyunfu.com` 进 API，两条 DNS 指向该 CLB 地址。证书要同时覆盖这两个域名。

共享模式要求 Service/Ingress Controller 至少为 v2.10.0，必须在创建 Ingress 时启用。已有非共享 Ingress 不得直接追加注解迁移。其他项目复用该 CLB 时也必须在创建时启用共享，域名和路径规则不得冲突。共享 HTTPS 监听器的默认域名由入口管理方统一维护，ERP 生产清单不声明 `defaultServer`。要求见[腾讯云多 Ingress 复用 CLB 文档](https://cloud.tencent.com.cn/document/product/457/127545)。

本地 overlay 未绑定生产共享 CLB，禁止将 local overlay 用于生产发布。Jenkins 发布入口为根目录 `Jenkinsfile.k8s`，执行要求见 [Jenkins 发布规范](jenkins/README.md)。

## 构建镜像

在仓库根执行。管理端的 `NEXT_PUBLIC_API_BASE_URL` 必须是浏览器能打开的 API 地址。Pod 里的 Service 名浏览器访问不到。生产用 `https://erp-api.fushangyunfu.com`。

```bash
docker build -t erp-api:dev -f backend/Dockerfile backend
docker build \
  --build-arg NEXT_PUBLIC_API_BASE_URL=https://erp-api.fushangyunfu.com \
  -t erp-client:dev \
  erp-client
```

本地集群需要能看到这两个镜像。kind 用 `kind load docker-image`，minikube 用 `minikube image load`。生产镜像沿用后端现有的 digest 发布方式，管理端镜像同样推送不可变 digest。

## 准备配置

`app.port` 保持 `10001`。示例里的 JWT secret 会被启动校验拒绝，必须换成至少 32 个随机字节。S3 的 bucket、密钥和 `public_base_url` 也要换成真实值。

```bash
mkdir -p deploy/k8s/secrets
cp deploy/k8s/examples/web-api-config.example.toml deploy/k8s/secrets/web-api-config.toml
kubectl create namespace prod --dry-run=client -o yaml | kubectl apply -f -
kubectl -n prod create secret generic erp-api-config \
  --from-file=config.toml=deploy/k8s/secrets/web-api-config.toml
kubectl apply -k deploy/k8s/overlays/local
```

`deploy/k8s/secrets/` 已在根 `.gitignore` 中。进程只在启动时读取配置。以后更新 Secret 后执行 `kubectl -n prod rollout restart deployment/erp-api`。

启用开发用 MongoDB 时，把 `database.uri` 设为：

```text
mongodb://mongo-0.mongo.prod.svc.cluster.local:27017/?replicaSet=rs0
```

```bash
kubectl apply -k deploy/k8s/optional/mongo
kubectl -n prod wait --for=condition=complete job/mongo-replicaset-init --timeout=180s
```

副本集成员主机名固定为 Pod DNS。集群域名不是 `cluster.local` 时，要同时改 `mongo.yaml` 里的初始化脚本和这条 URI。初始化 Job 的 Pod 模板不能原地修改；脚本改过之后先删除 Job 再 apply。

## 发布

```bash
kubectl apply -k deploy/k8s/overlays/local
kubectl apply -k deploy/k8s/overlays/production
```

生产域名是 `erp.fushangyunfu.com` 和 `erp-api.fushangyunfu.com`。发布前可将两条 DNS 指向共享 CLB `lb-gpk8k2ps` 的公网地址；apply 之后执行 `kubectl -n prod get ingress erp`，核对 ADDRESS 与该 CLB 一致，并检查 Ingress 事件是否存在同步错误。

```bash
kubectl -n kube-system get deploy l7-lb-controller
```

部分集群的 Ingress Controller 已托管或合并至 Service Controller；未找到 `l7-lb-controller` Deployment 不能单独判定控制器不可用。共享能力要求控制器版本至少为 v2.10.0，可查看 `kube-system` 中 `tke-service-controller-config`、`tke-ingress-controller-config` 的 VERSION，无法确认时通过 TKE 控制台或工单核实。不需要安装 nginx-ingress。

生产 overlay 里的 `registry.example.com` 和 `replace-with-release` 必须改成实际仓库和 digest 后再 apply。把 `images` 改成：

```yaml
images:
  - name: erp-api
    newName: registry.example.com/erp/erp-api
    digest: sha256:把后端发布清单里的 digest 填在这里
  - name: erp-client
    newName: registry.example.com/erp/erp-client
    digest: sha256:把管理端镜像的 digest 填在这里
```

没有 Ingress 时可以临时转发：

```bash
kubectl -n prod port-forward service/erp-api 10001:10001
kubectl -n prod port-forward service/erp-client 3000:3000
```

## 检查

- `GET /health` 返回 200 只表示进程已经开始监听。副本集校验和索引创建发生在监听之前；首次启动若索引较多，可能接近 startupProbe 的 10 分钟上限。
- `GET /ready` 在外部连接器仍是失败关闭时返回 503。当前组合根就是这个状态，所以探针使用 `/health`。
- 生产 overlay 的 `erp-api` 和 `erp-client` 各运行 1 个副本。登录和上传限流是进程内的，重启后重新计数。
- 单副本维护或故障期间可能中断服务。PDB 的 `minAvailable` 为 0，允许节点维护时驱逐；保留该资源以更新旧部署中的 PDB，避免仅从清单移除后旧规则残留。
- 管理端镜像不包含 `cli`。初始化或重置管理员仍在能访问同一 MongoDB 的环境执行 `cargo run -p cli -- init-admin`。
- 部署这版 JWT 账号版本校验后，已有后台 token 会失效，操作人员需要重新登录。
