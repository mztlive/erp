# ERP Kubernetes 部署合同

ERP 的生产、测试环境统一使用 `deploy/helm/erp` Chart，由根目录 `Jenkinsfile.k8s` 发布。不得继续使用 Kustomize 或 `kubectl apply` 管理已接管的 ERP 应用资源。其他应用接入现有基础设施时，按[新应用接入部署指南](../docs/k8s-application-deployment-guide.md)执行；ERP 的参数、凭据和发布步骤以 [Jenkins 发布执行规范](jenkins/README.md)为准。

## 1. 环境与归属

| 项目 | 生产 | 测试 |
| --- | --- | --- |
| `DEPLOY_ENV` | `production` | `test` |
| Namespace | `prod` | `test` |
| Helm release | `erp` | `erp` |
| 管理端 | `https://erp.fushangyunfu.com` | `https://erp-test.fushangyunfu.com` |
| API | `https://erp-api.fushangyunfu.com` | `https://erp-api-test.fushangyunfu.com` |
| 环境 values | `helm/erp/environments/production.json` | `helm/erp/environments/test.json` |
| 共享 CLB | `lb-gpk8k2ps` | `lb-gpk8k2ps` |
| 后端配置 Secret | `prod/erp-api-config` | `test/erp-api-config` |
| 证书 Secret | `prod/fsytsl-wsk87cm7` | `test/fsytsl-wsk87cm7` |

环境 JSON 是 Helm 原生支持的 values 输入，供 Helm 与 Python 标准库共同读取。域名、CLB、配置 Secret 引用只在环境文件维护；副本、资源额度、探针与通用默认值在 Chart 中维护。需要环境专属副本或资源额度时，在对应环境文件覆盖 `api`、`client` 或 `pdb` 字段。

同一命名空间仅允许一个 `erp` release。生产资源名称、Service 端口与 Deployment selector 保持现有值：后端 `erp-api`、前端 `erp-client`、Ingress 与 TkeServiceConfig `erp`。Chart 管理两组 ServiceAccount、Deployment、NodePort Service、PDB，以及 Ingress 和 TkeServiceConfig，共 10 个资源。

Chart 不管理 Namespace、MongoDB、S3、应用配置 Secret、证书 Secret、镜像拉取 Secret、共享 CLB 本身或集群 CRD。旧 `backend/Jenkinsfile` 与 Compose 链路保持独立，不得与 Helm 同时发布相同的 Kubernetes 工作负载。

## 2. 环境准备

集群管理员必须在首次发布前完成：

1. 创建对应命名空间；生产用 `prod`，测试用 `test`。
2. 在该命名空间创建 `erp-api-config`，键为 `config.toml`。参考 [后端配置模板](examples/web-api-config.example.toml)；端口必须为 `10001`，JWT 密钥必须替换为至少 32 个随机字节。
3. 生产与测试使用不同数据库及凭据、JWT 密钥、S3 bucket 或受权限隔离的前缀。不得向测试 Secret 复制整份生产配置。命名空间隔离不会自动隔离外部数据库和对象存储。
4. MongoDB 必须是副本集，成员地址必须从目标 Pod 可达。流水线不创建数据库、不清库、不执行种子或数据库迁移。
5. 在各自命名空间准备证书 Secret，包含 `qcloud_cert_id`，并确认覆盖该环境的两个域名。Secret 不跨命名空间引用；相同名称不代表测试环境已经有证书。
6. 将四个域名解析到共享 CLB，确认域名和路径未被其他应用占用。
7. 配置 TKE 免密拉取，或在各自命名空间创建镜像拉取 Secret，并通过 Jenkins `IMAGE_PULL_SECRET` 指定名称。免密拉取若限定 ServiceAccount，须覆盖两个命名空间中的 `erp-api` 和 `erp-client`。

使用腾讯云 `qcloud` Ingress、NodePort 和现有共享 CLB，不安装 ingress-nginx。共享模式必须在创建 Ingress 时启用；控制器须满足共享 CLB 的版本要求，至少 v2.10.0。已有非共享 Ingress 或绑定其他 CLB 的 Ingress 会阻断发布。不得通过删除线上 Ingress 强行通过检查。

共享 HTTPS 监听器的默认域名由入口管理方维护，Chart 不声明 `defaultServer`。入口配置依据[腾讯云多 Ingress 复用 CLB 文档](https://cloud.tencent.com.cn/document/product/457/127545)。

真实配置仅存放于已忽略的 `deploy/secrets/` 或受控配置系统；不得写入 values、Git、镜像或发布归档。应用日志输出 stdout/stderr。

## 3. 发布与配置更新

日常发布按 [Jenkins 发布执行规范](jenkins/README.md)执行，`DEPLOY_ENV` 默认 `test`；生产发布必须显式选择 `production`。环境文件修改必须随代码提交后发布。

前端 `NEXT_PUBLIC_API_BASE_URL` 在镜像构建时写入。发布脚本按环境推导 API 地址，不接受 Pod 环境变量替换。测试前端镜像不能直接提升到生产；必须为生产地址重新构建。镜像标签包含环境名，正式发布始终使用 `repository@sha256:...`。

后端只在启动时读取 `/app/config.toml`，通过 Secret 的 `subPath` 挂载。更新 Secret 后，必须执行对应命名空间的 `kubectl rollout restart deployment/erp-api`，或递增环境 values 中的 `api.configRevision` 后执行发布。Helm 不监视外部 Secret 内容变化。

默认每个工作负载 1 个副本，PDB `minAvailable: 0`，允许节点维护时驱逐。滚动更新可能临时增加 Pod，须预留容量；单副本维护或故障可能中断服务。修改副本数时必须同时检查 PDB 与应用后台任务并发约束。

API 的 `/health` 表示进程已经监听；副本集校验和索引创建发生在监听前，startupProbe 允许约 10 分钟。`/ready` 在外部连接器未配置时可能返回 503，不能替换当前探针。

## 4. 现有资源首次接管

已有 Kustomize 资源不能直接当作全新安装发布。生产首次切换必须先按 [Helm 接管执行规范](helm/migration.md)建立基线 release，再启用日常 Jenkins 发布。常规流水线不带 `--take-ownership`，不接管其他发布工具或其他 release 的资源。

首次接管前保留现有成功发布的镜像和清单，完成资源归属与入口核对。接管后停止使用旧的应用 Kustomize 清单；旧 `web-api` 资源如仍存在，必须单独核对业务归属及流量后清理，不能仅凭名称删除。

## 5. 外部数据库与验收边界

生产和测试均连接由数据库管理方独立维护的外部 MongoDB 副本集。应用部署目录不提供 MongoDB 安装清单；数据库生命周期、账号授权、备份与恢复由数据库管理方负责。发布前必须确认目标环境的数据库及凭据、网络可达性和副本集配置，不得将测试应用连接到生产业务库。

Helm 就绪、镜像核对及 HTTPS 探测通过后，仍须执行实际登录、权限、开单、审批、上传等业务验收。管理员初始化或密码重置使用可访问对应数据库的 CLI 环境，发布流水线不执行此类操作。
