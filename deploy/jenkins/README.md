# ERP TKE 发布执行规范

## 发布入口

Jenkins 任务必须使用仓库根目录 `Jenkinsfile.k8s`。旧 `backend/Jenkinsfile`、Compose 发布脚本保持独立。

固定发布目标：

| 对象 | 值 |
| --- | --- |
| Kubernetes 命名空间 | `prod` |
| 后端镜像仓库 | `fushangyun.tencentcloudcr.com/fushangyun/erp-api` |
| 前端镜像仓库 | `fushangyun.tencentcloudcr.com/fushangyun/erp` |
| 管理端域名 | `https://erp.fushangyunfu.com` |
| API 域名及前端构建参数 | `https://erp-api.fushangyunfu.com` |
| 后端配置 | Secret `web-api-config`，键 `config.toml` |
| CLB 证书 | Secret `fsytsl-wsk87cm7`，键 `qcloud_cert_id` |
| 共享 CLB | 手动创建的 `lb-gpk8k2ps`，生产 Ingress 创建时启用 group |

MongoDB 使用现有内网 CVM 的副本集，连接信息写入 `web-api-config`。流水线不得创建 MongoDB、重置业务数据、执行种子脚本或写入应用配置 Secret。

## Jenkins 配置

1. 在现有 Jenkins 新建唯一负责该环境的 Pipeline 任务，例如 `erp-production`。
2. Definition 选择 `Pipeline script from SCM`，SCM 选择 Git，填写 ERP 仓库、Git 凭据及生产分支。
3. Script Path 填 `Jenkinsfile.k8s`。发布分支由任务 SCM 配置确定；流水线使用 `checkout scm` 检出的提交，不另外切换分支。
4. 安装 Pipeline、Git、Credentials Binding 插件。任务运行于 `selfhost` 标签的可信 Agent。
5. 创建下表凭据。任务参数允许覆盖凭据 ID，禁止把凭据内容写进参数。

| 默认凭据 ID | Jenkins 类型 | 内容 |
| --- | --- | --- |
| `erp-tcr` | Username with password | 可向两个 TCR 仓库推送镜像的用户名、密码或访问令牌 |
| `erp-tke-kubeconfig` | Secret file | 可访问目标 TKE API Server 的 kubeconfig 文件 |

`KUBE_CONTEXT` 为可选参数，默认留空，自动使用上传 kubeconfig 的 `current-context`。只有需要覆盖文件默认选择时才填写。所有集群命令通过 `--kubeconfig` 明确指定上传文件，不读取 Agent 自身的配置，也不修改文件。文件缺失或未设置默认 context 且参数为空时，流水线停止并提示修正。可在可信终端执行 `kubectl --kubeconfig 文件路径 config get-contexts -o name` 查询可用名称，不得输出 kubeconfig 的完整内容。

Agent 必须安装 Git、Bash、grep、Python 3、kubectl（内置 Kustomize）、Docker Engine、系统级 Docker Buildx 插件及 curl 7.71 或以上。`RUN_QUALITY_CHECKS` 默认关闭，此时不要求主机安装 Cargo、Node.js 或 npm；后端编译及前端 `npm run build` 仍在 Docker 镜像构建中执行。

开启 `RUN_QUALITY_CHECKS` 时，Agent 还必须安装 Node.js 22.18 或以上的 22.x 版本、npm、Rustup，以及项目 `backend/rust-toolchain.toml` 所需的 nightly 和组件。后端主机检查还需要 C/C++ 编译器、CMake、pkg-config、OpenSSL 开发库。关闭开关只表示跳过质量门禁，不得将发布成功记作质量检查通过。

Agent 必须能够访问 Git、依赖镜像源、TCR、TKE API Server 和两个 HTTPS 业务域名。TKE 节点必须能够拉取对应平台的镜像。`IMAGE_PLATFORM` 默认 `linux/amd64`；使用 ARM 节点时必须选择 `linux/arm64`，并提供匹配构建节点或已配置的跨平台构建能力。

## 集群前置条件

发布前必须完成：

1. 创建 `prod` 命名空间。
2. 创建 `web-api-config`，写入完整生产配置。MongoDB 副本集成员地址必须从 Pod 可达；S3、JWT、服务端口等按 `deploy/README.md` 配置。
3. 在 `prod` 中准备覆盖两个域名的证书 Secret `fsytsl-wsk87cm7`。
4. 在 `prod` 中准备镜像拉取 Secret `tcr-pull`，类型为 `kubernetes.io/dockerconfigjson`；其凭据应具有两个镜像仓库的读取权限。若已配置 TKE 的 TCR 免密拉取，将 `IMAGE_PULL_SECRET` 参数留空。
5. 启用 TKE CLB Ingress 控制器及 `TkeServiceConfig` CRD，确认 Service/Ingress Controller 版本至少为 v2.10.0，并确认共享 CLB `lb-gpk8k2ps` 对目标集群可用。该 CLB 必须为手动创建，不得使用 TKE 自动创建的实例。保证有足够容量运行两个后端和两个前端副本，并容纳滚动更新的额外 Pod。
6. 为 kubeconfig 授予 `prod` 内 ServiceAccount、Deployment、Service、Ingress、PodDisruptionBudget、TkeServiceConfig 的 get/create/patch 权限，以及发布检查所需的 Deployment get/list/watch 权限。授予对前述指定 Secret 的 get 权限。流水线不创建 Namespace，不需要 Secret 写权限。

流水线访问 Secret 只输出对应键是否存在，不将其内容写入日志或发布归档。Secret 键存在不代表配置有效，应用启动与真实业务验收仍必须检查。

首次发布由流水线创建 `prod/erp` Ingress 并复用 `lb-gpk8k2ps`，无需提前手动创建 Ingress。两个域名的 DNS 必须指向该共享 CLB 的公网地址。若 DNS 尚未配置，工作负载可能已发布，但最终 HTTPS 检查会失败；必须完成 DNS 和证书配置后复核。禁止把该次失败认定为已自动回滚。

前置检查允许 `erp` Ingress 不存在；若已存在，则必须已开启 `ingress.cloud.tencent.com/enable-group: "true"` 且绑定 `lb-gpk8k2ps`。遇到非共享 Ingress、其他 CLB 或读取失败时，流水线停止，不执行 apply，不自动删除入口。共享模式无法通过追加注解应用到已有非共享 Ingress；迁移必须另行安排流量切换。此检查不验证 CLB 来源、控制器版本或其他项目的路由冲突，这些条件须在发布前确认。要求见[腾讯云官方说明](https://cloud.tencent.com.cn/document/product/457/127545)。

其他项目共享该 CLB 时，其 Ingress 也必须在首次创建时启用 group，且域名和路径规则不得与 ERP 冲突。共享监听器的默认域名由入口管理方统一维护；ERP 生产清单不设置 `defaultServer`。

## 执行与结果

提交新发布文件和已有 K8s/前端 Docker 配套文件后，从 Jenkins 构建任务运行。上传的 kubeconfig 已设置 `current-context` 时，无需填写 `KUBE_CONTEXT`；需要覆盖时使用 `Build with Parameters` 设置。`DEPLOY_TO_TKE` 默认开启；关闭时只构建推送与归档，不绑定集群凭据。`RUN_QUALITY_CHECKS` 独立控制质量检查，默认关闭。

执行顺序：

1. 清理本任务工作区并检出 SCM 提交。
2. 检查工具、必需 Secret 和指定 context。
3. 仅在 `RUN_QUALITY_CHECKS=true` 时执行发布脚本离线测试，以及后端格式、编译、Clippy、库单元测试、架构边界和权限漂移检查。
4. 仅在该开关开启时执行主机上的前端依赖安装、lint、TypeScript 和单元测试；前端镜像构建始终执行 `npm run build`。
5. 构建并推送两个镜像，标签为提交短 SHA 加构建编号；读取 Buildx 产生的镜像 digest。
6. 在临时目录渲染生产清单，将两个 Deployment 镜像写为 `repository@sha256:...`，配置拉取 Secret，并注入发布标识。不得修改仓库中的生产 overlay。
7. 归档 `release-artifacts`，执行 API Server dry-run，通过后正式 apply。
8. 等待两个 Deployment rollout，核对集群镜像与本次发布 digest，并检查 API `/health` 和前端 `/` 的 HTTPS 响应。

后端专用 `Dockerfile.web-api` 以 `backend/` 为上下文，预建 `/erp-client/lib`，供 `build.rs` 生成目录文件。前端镜像使用仓库中的生成物；开启质量检查时由权限漂移检查校验，关闭时不执行该校验。

`release.json` 记录提交、构建编号和两个镜像 digest；`manifests.yaml` 是本次完整发布清单。只有 rollout、镜像核对和 HTTPS 检查全部通过，才会生成 `deployment-status.txt`。这些检查不等价于登录、权限、业务事务和上传验收。

任一阶段失败必须将 Jenkins 构建标记为失败。apply 可能部分成功；流水线不自动回滚，不自动重试整次发布，不打印后端日志或真实配置。排障时根据集群资源状态确认实际影响。

## 回退

回退必须选定上一次成功构建归档的 `manifests.yaml`。使用同一目标 context 先执行服务端 dry-run，再 apply，并重新等待两个 Deployment rollout 及检查 HTTPS。不得仅回退一个镜像后宣称整次发布已恢复。

```bash
kubectl --context 目标context -n prod apply --dry-run=server -f 上次成功发布/manifests.yaml
kubectl --context 目标context -n prod apply -f 上次成功发布/manifests.yaml
kubectl --context 目标context -n prod rollout status deployment/web-api --timeout=900s
kubectl --context 目标context -n prod rollout status deployment/erp-client --timeout=900s
```

镜像与清单回退不会撤销数据库变化，也不会恢复 Secret 内容。旧版本必须与当前数据库和配置兼容。TCR 应保留成功版本 digest，禁止在可回退窗口内清理对应镜像。
