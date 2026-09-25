# ERP Helm 发布执行规范

## 1. 入口与环境

Jenkins 使用仓库根目录 `Jenkinsfile.k8s`，在 `selfhost` Agent 执行。`DEPLOY_ENV` 支持 `test` 与 `production`，默认 `test`；环境的命名空间、域名、Secret 引用和 CLB 由 `deploy/helm/erp/environments/<环境>.json` 读取，禁止通过额外命名空间参数绕过对应关系。

发布逻辑集中在 `release.sh`，由 Shell 调用 Docker、Helm 和 kubectl；`jq` 仅处理环境配置及命令输出的 JSON。Chart 打包、渲染、升级和版本历史直接使用 Helm 命令。

环境配置及集群准备要求见 [Kubernetes 部署合同](../README.md)。已有资源必须先按[接管执行规范](../helm/migration.md)建立 Helm 基线。

## 2. Jenkins 配置

1. 新建一个负责 ERP 两个环境的 Pipeline 任务，例如 `erp-k8s`。禁止另建任务并发发布同一个环境；`disableConcurrentBuilds()` 只约束同一任务。
2. 使用 `Pipeline script from SCM`，配置 ERP 仓库、Git 凭据和承载 `Jenkinsfile.k8s` 的分支，Script Path 填 `Jenkinsfile.k8s`。任务配置的分支只决定流水线定义。`ACTION=deploy` 时检出参数 `GIT_REF`；`ACTION=rollback` 时检出任务配置的分支，不使用 `GIT_REF`。
3. 安装 Pipeline、Git、Git Parameter、Credentials Binding 插件；准备以下凭据。参数只能填写凭据 ID，不得填写凭据内容。
4. 将含本流水线的提交推送到任务配置的分支后，使用 `Build with Parameters`。首次加载新参数时重新打开任务再构建。生产必须显式选择 `production`。

| 参数 | 默认值与执行规则 |
| --- | --- |
| `ACTION` | `deploy`。`rollback` 按 Helm revision 回退，不构建镜像，且要求 `DEPLOY_TO_TKE=true` |
| `GIT_REF` | `origin/main`。仅 `deploy` 使用，可选远程分支或 tag |
| `ROLLBACK_REVISION` | 空。仅 `rollback` 使用，填正整数；来源是该环境一次已生成 `deployment-status.txt` 的构建里的 `helm-history.json` |
| `DEPLOY_ENV` | `test`；生产必须显式选择 `production` |
| `TCR_CREDENTIALS_ID` | `tcr`，Username with password，可推送两个业务仓库，并可读取 `base` 下的构建镜像 |
| `KUBECONFIG_CREDENTIALS_ID` | `tke-kubeconfig`，Secret file，选择对目标环境有权限的文件 |
| `KUBE_CONTEXT` | 空，使用上传文件的 current-context；可显式覆盖 |
| `IMAGE_PULL_SECRET` | 空表示使用 TKE 免密拉取；否则指定目标命名空间已有 Secret |
| `IMAGE_PLATFORM` | `linux/amd64`，ARM 节点选择 `linux/arm64` |
| `RUN_QUALITY_CHECKS` | 默认 false，只控制前后端主机质量门禁 |
| `DEPLOY_TO_TKE` | 默认 true；false 时只构建、推送与归档，不使用集群凭据 |

镜像仓库保持 `fushangyun.tencentcloudcr.com/fushangyun/erp-api` 和 `fushangyun.tencentcloudcr.com/fushangyun/erp`。标签为 `<提交短 SHA>-<环境>-<构建编号>`，避免两个环境覆盖相同标签；工作负载使用不可变 digest。

基础镜像统一从 `fushangyun-vpc.tencentcloudcr.com/base` 拉取。三个 Dockerfile 固定 Rust、Debian、Node 和 Dockerfile frontend 的 digest；`release.sh` 固定 BuildKit 的 digest，并在独立 `DOCKER_CONFIG` 中分别登录成品仓库域名和 VPC 域名。Agent 主机和 BuildKit 容器必须能够解析并访问 VPC 域名；TKE 免密拉取不能替代 Agent 的登录凭据。

基础镜像升级必须先同步到 TCR、记录目标 digest，再更新对应引用。`scripts/sync-erp-images.sh` 同步五个基础镜像，`scripts/sync-buildkit-image.sh` 单独同步 BuildKit；同步成功不等于构建验收完成。切换后首次 Jenkins 验证必须设置 `DEPLOY_TO_TKE=false`，确认固定镜像可拉取、前后端可构建推送后再发布。

手工运行 Dockerfile 或独立的旧 Compose 构建入口前，必须执行 `docker login fushangyun-vpc.tencentcloudcr.com` 并具备 VPC 访问能力。本地 MongoDB 的 Compose 与启动脚本默认使用自有 TCR 普通域名及固定 digest，需登录 `fushangyun.tencentcloudcr.com`；内网环境可通过 `ERP_DEV_MONGO_IMAGE` 指定同一 digest 的 VPC 地址。

所有集群命令显式使用上传的 kubeconfig、选定 context 和环境 namespace，不读取 Agent 默认 kubeconfig。文件缺失、无可用 context、网络或权限错误必须停止。

## 3. Agent 与权限

Agent 必须安装 Git、Bash、grep、jq 1.6+、kubectl、Helm 4.x、Docker Engine、系统级 Buildx 插件及 curl 7.71+。发布和离线测试均不依赖 Python。Helm 必须固定经过验证的补丁版本；当前验证版本为 4.1.3，流水线拒绝 Helm 3。升级失败回退采用 Helm 4 的 `--rollback-on-failure`，参数依据 [Helm upgrade 文档](https://helm.sh/docs/helm/helm_upgrade/)。

`RUN_QUALITY_CHECKS=true` 时，还须安装 Node.js 22.18+ 的 22.x 版本、npm、Rustup 及后端指定 nightly/组件，以及 C/C++ 编译器、CMake、pkg-config、OpenSSL 开发库。关闭质量门禁不跳过镜像内编译，也不跳过部署脚本离线测试。

Agent 必须能够访问 Git、依赖源、TCR、目标 TKE API Server 和所选环境的两个 HTTPS 域名。构建平台必须与节点架构一致。

集群凭据必须具备目标命名空间中 ServiceAccount、Deployment、Service、Ingress、PDB、TkeServiceConfig 的发布与读取权限，以及等待 Pod/ReplicaSet 等状态所需的读取权限。Helm 默认将 release 历史保存在 Secret，需要 Secret 的 get/list/watch/create/update/patch/delete 权限。应用配置、证书和拉取 Secret 由管理员管理，发布代码只校验其数据键存在，不打印内容、不更新这些 Secret。不得给流水线 Namespace、CRD 或其他集群级资源的写权限。

## 4. 发布顺序与产物

1. 清理任务工作区。`deploy` 检出 `GIT_REF`，`rollback` 检出任务配置分支。随后校验工具版本和环境映射；`rollback` 同时校验 revision 为正整数。
2. 始终执行 `bash deploy/jenkins/test_release.sh`。测试使用真实的本地 Helm 和 Docker、kubectl、HTTP 命令替身，不连接 Docker、集群或业务服务；测试工作区在临时目录，退出时清理。
3. `deploy` 且要发布到集群，或 `rollback` 时，检查现有 Ingress 的共享模式和 CLB 归属，以及配置、证书、拉取 Secret 所需的数据键。
4. 仅 `deploy` 按质量开关执行后端格式、编译、Clippy、库单元测试、边界与权限漂移检查，以及前端 lint、TypeScript 和单元测试。不得新增或执行后端集成测试。
5. 仅 `deploy`：使用隔离的 Docker 登录目录构建推送两个镜像；结束时只清理凭据目录。Buildx builder 按平台保留为 `erp-<平台>`，cargo 缓存留在该 builder 的状态卷中。记录目录默认是 `~/.local/state/erp-buildx`，可用 `BUILDX_CONFIG` 覆盖。固定的 BuildKit 镜像变化时才重建 builder。前端构建地址从所选环境读取。
6. 构建成功后，在同一 Shell 环境内读取并校验 Buildx digest，将环境 values、两个镜像地址及发布标识合并为 `values-release.json`，直接执行 Helm lint、package、template，归档完整发布包。构建开始时清除旧产物，失败不得继续发布。
7. 核对归档 values 与当前环境、域名和 Secret 引用一致，再用归档 Chart 和 values 重新渲染清单；由 Chart schema 校验参数。执行 API Server 清单 dry-run 与 Helm 服务端 dry-run，通过后用同一 Chart 包和 values 升级。
8. Helm 等待资源就绪，超时为 15 分钟；升级失败请求回退到上一成功 release。随后再次等待两个 Deployment rollout，核对镜像，检查 API `/health` 和管理端 `/` 的公网 HTTPS 响应。

发布归档保留：

| 文件 | 用途 |
| --- | --- |
| `api-build.json`、`web-build.json` | Buildx 镜像 digest |
| `chart.tgz` | 本次使用的完整 Chart，不依赖后续 Git 工作区 |
| `values-release.json` | `deploy` 时为本次环境参数、镜像 digest、拉取 Secret 名称，`releaseId` 为完整提交 SHA、环境和构建编号。`rollback` 时为目标 revision 已保存的 values |
| `manifests.yaml` | 本次渲染清单，供审查；不得直接 apply 到已由 Helm 管理的环境 |
| `helm-history.json` | Helm 升级成功后读取的 release 历史 |
| `deployments.json` | 部署后镜像核对证据 |
| `deployment-status.txt` | 仅在 rollout、镜像核对和公网检查全部通过时生成 |

任一阶段失败，Jenkins 必须标记失败。Helm 成功后发生的镜像核对或公网探测失败，不触发 Helm 自身的自动回退，必须排查并按下一节回退。自动回退也可能失败，应查看 Helm 和集群实际状态；不得将 Jenkins 失败直接解释为已恢复旧版。

首次安装不存在上一版本。正常新环境首次安装失败时，Helm 的失败处理可能卸载新安装资源；旧生产资源接管不得使用常规安装流程。任何回退均不恢复外部 Secret 或撤销数据库变化。

## 5. 查询与回退

操作人必须先从本任务已生成 `deployment-status.txt` 的成功构建中选择目标版本，再用对应构建的 `helm-history.json` 确定 release revision。仅 `helm history` 显示 deployed 不足以证明公网和业务检查通过。

Jenkins 回退：`ACTION=rollback`，`DEPLOY_ENV` 选目标环境，`ROLLBACK_REVISION` 填该 revision。流水线拒绝空值、当前 `deployed`、状态不是 `superseded`、以及镜像 digest 无效的 revision。回退不构建镜像，不恢复配置 Secret 或数据库。完成后同样等待 rollout，并按该 revision 保存的 values 核对两个镜像和公网入口。

Jenkins 不可用时，在受控终端执行相同回退。替换 kubeconfig 路径和 context；测试用 `test`，生产用 `prod`。不得省略命名空间。

```bash
helm --kubeconfig /path/to/kubeconfig --kube-context 目标context -n test history erp
helm --kubeconfig /path/to/kubeconfig --kube-context 目标context -n test rollback erp 目标revision --history-max 20 --wait --timeout 15m
kubectl --kubeconfig /path/to/kubeconfig --context 目标context -n test rollout status deployment/erp-api --timeout=900s
kubectl --kubeconfig /path/to/kubeconfig --context 目标context -n test rollout status deployment/erp-client --timeout=900s
```

回退后核对两组镜像与目标成功构建归档一致，检查所选环境两个 HTTPS 入口，再执行业务验收。Helm 历史上限为 20，Jenkins 构建/归档保留最近 20 次；不得承诺超出保留窗口的 revision 可直接回退。需要长期保留的版本必须另行保存归档，并确保 TCR 未清理其 digest。

若 revision 已被清理，使用保存的 `chart.tgz` 和 `values-release.json` 执行经过审核的 `helm upgrade --install` 恢复该版本，先完成环境核对、服务端 dry-run、配置与数据兼容性检查。不得从测试归档恢复生产。
