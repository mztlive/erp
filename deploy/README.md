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
| Nacos Namespace UUID | `8888c735-1d29-4765-ab7f-70100a888479` | `ccf7ec38-1d60-407e-bf2c-7c4654c481d0` |
| Nacos Group / Data ID | `DEFAULT_GROUP / erp` | `DEFAULT_GROUP / erp` |
| Nacos 只读账号 | `prod` | `test` |
| Nacos 凭据 Secret | `prod/erp-api-config` | `test/erp-api-config` |
| 证书 Secret | `prod/fsytsl-wsk87cm7` | `test/fsytsl-wsk87cm7` |

环境 JSON 是 Helm 原生支持的 values 输入，发布脚本通过 `jq` 读取其中的域名和配置引用。域名、CLB、配置 Secret 引用只在环境文件维护；副本、资源额度、探针与通用默认值在 Chart 中维护。需要环境专属副本或资源额度时，在对应环境文件覆盖 `api`、`client` 或 `pdb` 字段。

同一命名空间仅允许一个 `erp` release。生产资源名称、Service 端口与 Deployment selector 保持现有值：后端 `erp-api`、前端 `erp-client`、Ingress 与 TkeServiceConfig `erp`。Chart 管理两组 ServiceAccount、Deployment、NodePort Service、PDB，以及 Ingress 和 TkeServiceConfig，共 10 个资源。

Chart 不管理 Namespace、MongoDB、S3、应用配置 Secret、证书 Secret、镜像拉取 Secret、共享 CLB 本身或集群 CRD。旧 `backend/Jenkinsfile` 与 Compose 链路保持独立，不得与 Helm 同时发布相同的 Kubernetes 工作负载。

## 2. 环境准备

集群管理员必须在首次发布前完成：

1. 创建对应命名空间；生产用 `prod`，测试用 `test`。
2. 按 [Nacos 环境配置接入与权限管理规范](https://outline.fushangyunfu.com/doc/nacos-vxFwmIgO8r)向运维登记本表标识。将[后端配置模板](examples/web-api-config.example.toml)填写为完整 TOML，发布到对应 Namespace 的 `DEFAULT_GROUP / erp`，Data ID 无后缀。端口必须为 `10001`，JWT 密钥至少 32 个随机字节。若目标库尚无 `admin`，可配置 `[bootstrap].initial_admin_password`（6 到 32 个字符）；已有账号不改密码。首次登录确认后，从 Nacos 删除该字段并重启 API。
3. 运维在应用命名空间创建或更新 `erp-api-config`，保留其他键并写入非空 `NACOS_USERNAME`、`NACOS_PASSWORD`。测试使用 `test`，生产使用 `prod`；禁止管理员或 Seata 账号。Chart 将两键映射到 `NACOS_CLIENT_USERNAME`、`NACOS_CLIENT_PASSWORD`。应用 Pod 必须能解析 `nacos.infra.svc.cluster.local` 并访问 `8848/TCP`、`9848/TCP`。两环境须使用不同数据库及凭据、JWT 密钥、S3 bucket 或受权限隔离的前缀，不得复制整份生产配置到测试。
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

后端从 Nacos 读取并校验完整 TOML，首次连接与每次读取均设 30 秒超时。启动读取失败必须退出，不回退到文件或磁盘缓存。运行中每 10 秒刷新，读取或校验失败保留上一次有效快照。JWT 密钥可热更新；端口、数据库、RBAC、S3 客户端和管理员初始化属于启动期资源，修改后必须重启。

正式配置变更必须登记 Nacos 版本或摘要和兼容镜像，再递增环境 JSON 的 `api.configRevision` 并发布，以重建全部启动期资源。轮换环境账号密码须同步全部消费应用的 Secret 并重启。Helm 不监视外部 Secret 或 Nacos 内容变化。发布归档只保存地址、Namespace、Group、Data ID、Secret 名称和 revision，不保存密码或配置全文。

从文件模式切换前，必须保留历史 `erp-api-config` Secret 的 `config.toml` 键，并登记保留期限和兼容镜像。回滚 Nacos 版本前，先核对或恢复兼容目标镜像的 Nacos 配置；回滚历史文件版本前，确认旧文件 Secret 仍存在。流水线按目标 revision 的配置来源检查所需键，Helm 回滚不会恢复 Nacos 内容、Secret 或数据库。

默认每个工作负载 1 个副本，PDB `minAvailable: 0`，允许节点维护时驱逐。滚动更新可能临时增加 Pod，须预留容量；单副本维护或故障可能中断服务。修改副本数时必须同时检查 PDB 与应用后台任务并发约束。

API 的 `/health` 表示进程已经监听；副本集校验和索引创建发生在监听前，startupProbe 允许约 10 分钟。`/ready` 在外部连接器未配置时可能返回 503，不能替换当前探针。

## 4. 现有资源首次接管

已有 Kustomize 资源不能直接当作全新安装发布。生产首次切换必须先按 [Helm 接管执行规范](helm/migration.md)建立基线 release，再启用日常 Jenkins 发布。常规流水线不带 `--take-ownership`，不接管其他发布工具或其他 release 的资源。

首次接管前保留现有成功发布的镜像和清单，完成资源归属与入口核对。接管后停止使用旧的应用 Kustomize 清单；旧 `web-api` 资源如仍存在，必须单独核对业务归属及流量后清理，不能仅凭名称删除。

## 5. 外部数据库与验收边界

生产和测试均连接由数据库管理方独立维护的外部 MongoDB 副本集。应用部署目录不提供 MongoDB 安装清单；数据库生命周期、账号授权、备份与恢复由数据库管理方负责。发布前必须确认目标环境的数据库及凭据、网络可达性和副本集配置，不得将测试应用连接到生产业务库。

Helm 就绪、镜像核对及 HTTPS 探测通过后，仍须执行实际登录、权限、开单、审批、上传等业务验收。管理员初始化或密码重置使用可访问对应数据库的 CLI 环境，发布流水线不执行此类操作。

## 6. Nacos 接入验收

发布前完成配置解析和应用校验；发布后使用实际应用 Pod、实际 SDK 验证读取。必须分别记录：目标配置读取成功、跨环境读取拒绝、无额外写授权、8848/9848 链路、配置生效及关键业务验收。写入拒绝测试只能使用专用验收条目。记录环境、账号名、Namespace / Group / Data ID、配置版本或摘要、镜像、时间、执行人和结果，不记录密码及配置全文。

本地调试先运行 `kubectl -n infra port-forward --address 127.0.0.1 svc/nacos 8848:8848 9848:9848`，再按 [Config 接入说明](../backend/config/README.md)启动。调试完成停止转发；本机通过不能替代 Pod 验收。仓库离线检查不得连接真实 Nacos、MongoDB 或 S3。
