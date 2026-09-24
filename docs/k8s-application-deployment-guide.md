# 使用现有 Jenkins 和 Helm 发布服务到 TKE

供开发同事将自己的应用部署到现有 TKE 使用。复用现有 Jenkins、TCR、TKE 和共享 CLB，不重新搭建基础设施。发布链路统一为：**代码 → 构建镜像 → 推送 TCR → Helm 安装或升级 → 验证服务**。

以下以 `sample-service` 演示，执行时替换为自己的服务名。前端与后端均适用；需要一起发布的多个工作负载可以放入同一个 Chart，独立发布的应用分别使用自己的 Chart 和 release。ERP 本项目直接使用[已有发布规范](../deploy/jenkins/README.md)，不套用本指南的示例字段。

## 1. 确定应用和环境

| 配置 | 值 |
| --- | --- |
| Jenkins Agent | `selfhost` |
| Jenkins TCR 凭据 ID | `tcr` |
| Jenkins kubeconfig 凭据 ID | `tke-kubeconfig` |
| 示例镜像仓库 | `fushangyun.tencentcloudcr.com/fushangyun/sample-service` |
| 共享 CLB | `lb-gpk8k2ps` |
| 证书 Secret 名称 | `fsytsl-wsk87cm7`，必须确认目标命名空间中存在且覆盖应用域名 |

在 TCR 的 `fushangyun` 命名空间创建自己的镜像仓库。约定两个环境：

| 配置 | 测试 | 生产 |
| --- | --- | --- |
| 发布参数 `DEPLOY_ENV` | `test`，默认值 | `production`，显式选择 |
| Kubernetes 命名空间 | `test` | `prod` |
| 示例 Helm release | `sample-service` | `sample-service` |
| 示例域名 | `sample-test.fushangyunfu.com` | `sample.fushangyunfu.com` |
| 环境 values | `values-test.yaml` | `values-production.yaml` |

Helm release 是一次应用安装的名称，两个命名空间可以使用相同 release 名。同一命名空间不得与其他应用重名。Deployment、容器、ServiceAccount、Service、Ingress 等资源统一使用自己的应用名称，不保留被复制项目的名称。

每个环境的数据库、数据库凭据、应用密钥及对象存储空间必须独立配置。命名空间隔离不会自动隔离外部数据库；禁止直接复制生产配置作为测试配置。

## 2. 在 Jenkins Agent 安装 Helm

在运行 `selfhost` Agent 的服务器上安装 Helm 4.x，并固定团队验证过的版本。以下使用本仓库离线验证过的 `v4.1.3`：

```bash
curl -fsSL https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-4 -o get-helm-4.sh
bash get-helm-4.sh --version v4.1.3
command -v helm
helm version --short
```

脚本默认安装到 `/usr/local/bin/helm`。必须在 Jenkins 实际执行 Shell 的环境中确认命令可用；若 SSH 终端可用而 Jenkins 找不到，给 Agent 的 PATH 加入 `/usr/local/bin`。安装方式见 [Helm 官方说明](https://helm.sh/docs/intro/install/)。

Agent 还须具备 Git、Bash、Docker/Buildx、kubectl、curl，以及项目构建和脚本要求的工具，并能访问代码仓库、TCR、TKE API Server 和应用入口。Helm 使用 kubeconfig 访问集群，安装 CLI 不会自动授予集群权限。

## 3. 在应用仓库准备 Chart

项目必须提交以下文件，名称可按项目调整，但 Jenkins 中引用的路径必须一致：

```text
Dockerfile
Jenkinsfile.k8s
deploy/helm/sample-service/
├── Chart.yaml
├── values.yaml
├── values-test.yaml
├── values-production.yaml
└── templates/
    ├── deployment.yaml
    ├── serviceaccount.yaml
    ├── service.yaml
    └── ingress.yaml
```

`Chart.yaml` 至少包含：

```yaml
apiVersion: v2
name: sample-service
version: 0.1.0
type: application
```

可以用 `helm create deploy/helm/sample-service` 生成初始结构，再按自己的应用修改。生成的默认 Chart 不能直接作为本指南的 TKE 发布文件；必须调整 values 字段、模板、入口与探针。

下面约定一组示例 `values.yaml` 字段。**字段只有被模板引用才会生效；这些字段不是 Helm 自动提供的应用配置。**

```yaml
image: "" # 发布时填写完整 repository@sha256:...，模板必须拒绝空值
replicaCount: 1
containerPort: 8080
healthPath: /health
imagePullSecret: "" # 空表示使用已配置好的 TKE 免密拉取
config:
  existingSecret: sample-service-config
  key: config.yaml
  mountPath: /app/config.yaml
resources:
  requests: {cpu: 100m, memory: 256Mi}
  limits: {cpu: "1", memory: 1Gi}
service:
  type: NodePort
  port: 8080
ingress:
  enabled: true
  host: ""
  clbId: lb-gpk8k2ps
  tlsSecret: fsytsl-wsk87cm7
```

两个环境文件只覆盖差异项，例如：

```yaml
# values-test.yaml
ingress:
  host: sample-test.fushangyunfu.com
```

```yaml
# values-production.yaml
ingress:
  host: sample.fushangyunfu.com
```

模板必须满足以下要求：

| 模板 | 必须实现的内容 |
| --- | --- |
| Deployment | 镜像引用 `{{ required "image 必须填写 digest 镜像地址" .Values.image }}`；绑定副本、端口、资源额度、探针和配置挂载 |
| ServiceAccount | 使用本应用账号；启用 TCR 免密拉取时确认该账号已被授权 |
| Service | selector 与 Pod 标签一致；`targetPort` 指向实际容器端口 |
| Ingress | 根据 `ingress.enabled` 创建；域名、CLB 和证书引用 values，后端指向本应用 Service |
| 所有资源 | 明确归属本应用和 release，使用 `.Release.Namespace`；首次发布后保持 Deployment selector 稳定 |

需要接收请求的应用必须监听 `0.0.0.0`。例如程序监听 `8080`，容器端口、Service 目标端口和探针端口必须一致；探针路径必须是应用实际提供的接口。资源额度和启动等待时间按应用调整，滚动更新须预留额外 Pod 容量。

日志输出 `stdout` / `stderr`，关闭文件日志。密码、令牌和完整运行配置存入外部 Secret，Chart 只引用名称；不得放入 values、镜像、Git 或发布归档。应用必须按挂载路径读取配置；无配置文件需求时删除相应 values 和挂载模板。需要持久保存的数据不得放在 Pod 临时目录。

## 4. 准备配置与访问入口

在 TKE 控制台选择目标命名空间，完成以下准备。`test` 和 `prod` 分别操作，不能跨命名空间引用 Secret。

| 对象 | 操作 |
| --- | --- |
| 命名空间 | 由管理员预先创建 `test`、`prod`；应用 Chart 不创建共享命名空间 |
| 运行配置 | 创建 `sample-service-config`，示例键为 `config.yaml`，值为该环境的完整配置 |
| 证书 | 准备覆盖该环境域名的 `fsytsl-wsk87cm7`；沿用 TKE 证书引用方式时包含 `qcloud_cert_id` |
| 镜像拉取 | 配置 TKE 免密拉取，或创建拉取 Secret 并在模板中绑定 `imagePullSecrets` |
| 外部依赖 | 确认数据库、副本集成员、对象存储等地址从该环境 Pod 可达 |

需要 HTTP/HTTPS 域名访问时，Ingress 使用 `qcloud`，并在首次创建时包含以下配置。模板中的 Service 名称和端口须指向自己的应用：

```yaml
# templates/ingress.yaml 中的对应字段，需合入完整 Ingress 模板
metadata:
  annotations:
    kubernetes.io/ingress.class: qcloud
    ingress.cloud.tencent.com/enable-group: "true"
    kubernetes.io/ingress.existLbId: {{ .Values.ingress.clbId | quote }}
spec:
  ingressClassName: qcloud
  tls:
    - hosts:
        - {{ .Values.ingress.host | quote }}
      secretName: {{ .Values.ingress.tlsSecret | quote }}
```

Ingress 的 `spec.rules[].host` 同样引用 `ingress.host`。将测试、生产域名分别解析到共享 CLB `lb-gpk8k2ps` 的公网地址，确认域名和路径未被其他应用使用，证书覆盖对应域名。Ingress 由 Helm 创建，不必提前手工创建，也不需要为每个应用购买新的 CLB。

共享模式必须在创建 Ingress 时启用；已有非共享 Ingress 不能仅追加注解切换。应用只管理自己的转发规则；需要自定义 CLB 健康检查时，在 Chart 中增加本应用的 TkeServiceConfig 并从 Ingress 引用。不得修改共享监听器的 `defaultServer`。

只供集群内调用的服务使用 `ClusterIP`，关闭 Ingress；不接收请求的任务无需 Service 或 Ingress。数据库、共享证书、CLB 本身和集群 CRD 不随应用 release 安装或卸载。

## 5. 配置 Jenkins 发布流程

在现有 Jenkins 新建 Pipeline 任务，例如 `sample-service-k8s`，选择 `Pipeline script from SCM`，填写自己的仓库、Git 凭据和发布分支，Script Path 填 `Jenkinsfile.k8s`。将所有发布文件提交并推送到该分支后执行。

项目流水线必须实现以下规则：

1. 在 `selfhost` 上执行，禁用同一任务并发构建；不得再配置其他任务并发操作相同 namespace/release。
2. 提供 `DEPLOY_ENV` 参数，默认 `test`；仅允许 `test → test`、`production → prod`，同步选择对应 values 文件。
3. 从 Jenkins `tcr` 凭据读取推送用户名和密码，从 `tke-kubeconfig` 文件凭据读取 kubeconfig。显式选择文件中的 context，禁止回退到 Agent 默认配置。
4. 检查工具、运行配置、证书、镜像拉取条件及现有 Ingress 的 CLB 归属；按应用要求执行质量检查。
5. 构建并推送镜像，建议标签采用 `<提交短 SHA>-<环境>-<构建编号>`。从 Buildx `--metadata-file` 结果读取 `containerimage.digest`，生成本次发布的完整镜像地址。
6. 生成 `release-artifacts/values-release.yaml`，至少包含 `image: 仓库地址@sha256:实际digest`；需要时填写 `imagePullSecret`。禁止用 `latest` 代替发布 digest。前端若将 API 地址写入构建产物，必须按目标环境重新构建。
7. 执行下述校验、打包和 Helm 发布步骤，然后验证实际镜像和应用入口，全部通过才标记发布成功。
8. 归档提交号、环境、镜像 digest、Chart 包、环境 values、本次发布 values、渲染清单、Helm revision 和验证结果。归档不得包含真实配置或 kubeconfig。

**这些步骤需要在自己项目的 Jenkinsfile 或发布脚本中实现。只创建同名文件不能完成发布；不得原样调用 ERP 专用脚本去部署其他应用。**

以下是发布脚本中的 Helm 部分。在 Bash 中执行，前提是镜像已推送、发布 values 已生成，且 Jenkins 已绑定 `KUBECONFIG`。`KUBE_CONTEXT` 可留空以使用上传文件的 current-context：

```bash
set -euo pipefail
: "${DEPLOY_ENV:?必须选择 test 或 production}"
: "${KUBECONFIG:?必须绑定 Jenkins kubeconfig 文件凭据}"
test -f "$KUBECONFIG"
case "$DEPLOY_ENV" in
  test) namespace=test ;;
  production) namespace=prod ;;
  *) echo '不支持的环境' >&2; exit 1 ;;
esac
context="${KUBE_CONTEXT:-}"
if [ -z "$context" ]; then
  context="$(kubectl --kubeconfig "$KUBECONFIG" config current-context)"
fi
test -n "$context"

release=sample-service
chart=deploy/helm/sample-service
mkdir -p release-artifacts
cp "$chart/values-$DEPLOY_ENV.yaml" release-artifacts/values-environment.yaml
test -s release-artifacts/values-release.yaml
values=(-f release-artifacts/values-environment.yaml -f release-artifacts/values-release.yaml)
kube=(kubectl --kubeconfig "$KUBECONFIG" --context "$context" -n "$namespace")
helm_target=(helm --kubeconfig "$KUBECONFIG" --kube-context "$context" -n "$namespace")

helm lint "$chart" --strict -n "$namespace" "${values[@]}"
package_dir="$(mktemp -d)"
trap 'rm -rf "$package_dir"' EXIT
helm package "$chart" --destination "$package_dir"
mv "$package_dir"/*.tgz release-artifacts/chart.tgz
helm template "$release" release-artifacts/chart.tgz -n "$namespace" "${values[@]}" \
  > release-artifacts/manifests.yaml

# 两次 dry-run 均通过后，才执行正式升级。
"${kube[@]}" apply --dry-run=server -f release-artifacts/manifests.yaml >/dev/null
"${helm_target[@]}" upgrade --install "$release" release-artifacts/chart.tgz \
  "${values[@]}" --reset-values --dry-run=server --hide-secret >/dev/null
"${helm_target[@]}" upgrade --install "$release" release-artifacts/chart.tgz \
  "${values[@]}" --reset-values --wait --rollback-on-failure --timeout 15m --history-max 20
"${helm_target[@]}" history "$release" -o json > release-artifacts/helm-history.json
```

`kubectl apply --dry-run=server` 只校验、不写入资源；正式发布统一由 Helm 执行，不再对同一应用执行普通 `kubectl apply`。Helm 升级与回退参数见[官方命令说明](https://helm.sh/docs/helm/helm_upgrade/)。

发布凭据需要目标命名空间中应用资源的发布权限、观察工作负载状态的权限，以及 Helm release 历史 Secret 的读写权限。原先只有 `kubectl apply` 权限的凭据不一定满足 Helm 要求。共享命名空间、CRD 和基础设施由管理员管理。

## 6. 发布后验证与回退

以下命令接续上一节的同一 Bash 会话。示例约定 Deployment 和容器均名为 `sample-service`，不同命名必须同步调整：

```bash
"${helm_target[@]}" status "$release"
"${kube[@]}" rollout status deployment/sample-service --timeout=900s
"${kube[@]}" get deployment sample-service \
  -o jsonpath='{.spec.template.spec.containers[?(@.name=="sample-service")].image}'
"${kube[@]}" logs deployment/sample-service -c sample-service --tail=100
```

发布验证必须确认工作负载就绪且无持续重启、实际镜像与本次 digest 一致、依赖可访问，并从真实入口验证功能。使用域名时，还须检查 HTTPS 和 CLB 后端健康。无终端权限的同事在腾讯云控制台查看，涉及发布和回退的操作交由有权限人员执行。

更新代码或 Chart 后，重新执行同一 Jenkins 任务。更新外部 Secret/ConfigMap 后，按程序加载方式生效；环境变量或 `subPath` 文件挂载通常需要重建 Pod：

```bash
"${kube[@]}" rollout restart deployment/sample-service
"${kube[@]}" rollout status deployment/sample-service --timeout=900s
```

回退时，从 Jenkins 中选择已通过发布验证的构建，再根据其归档的 `helm-history.json` 确定目标 revision；不得只看 Helm 的 deployed 状态判断业务已经正常。确认旧版本兼容当前配置和数据后，执行：

```bash
"${helm_target[@]}" history "$release"
# 将 3 替换为已经核对的目标 revision。
"${helm_target[@]}" rollback "$release" 3 --wait --timeout 15m
"${kube[@]}" rollout status deployment/sample-service --timeout=900s
```

回退后再次核对镜像、入口和业务功能。不要下载旧 `manifests.yaml` 后直接 apply；它用于审查，Helm 管理的资源应通过 Helm 回退。

`--rollback-on-failure` 针对 Helm 安装/升级阶段的失败。后续公网探测或业务验证失败不会自动触发该回退；首次安装也不存在上一版本，失败处理可能卸载新建资源。任何回退都不会恢复外部 Secret 或撤销数据库变化。

示例保留 20 个 Helm revision，镜像和 Jenkins 归档也须覆盖约定的回退窗口。revision 已清理时，用受控归档中的 Chart 包、环境 values 和发布 values 执行重新升级，先完成环境核对和 dry-run。

## 7. 已有应用迁移与排错

已有应用由 kubectl/Kustomize 管理时，首次切换必须先备份当前资源，确认归属，保持资源名称、Deployment selector、Service 端口和入口一致，使用当前成功版本的镜像建立 Helm 基线，再开放普通 Jenkins 升级。

日常流水线不得附加 `--take-ownership`。一次性接管仅限已经确认属于本应用的资源，不能覆盖其他 release。首次接管不要使用失败卸载选项，也不得通过 `helm uninstall` 清理失败记录后重试。ERP 的具体命令见[首次接管规范](../deploy/helm/migration.md)；其他项目必须按自己的资源清单适配，不能直接复制 ERP 资源名。

| 错误 | 检查项 |
| --- | --- |
| `helm: command not found` | 是否装在实际执行任务的 Agent 上，以及 Agent 的 PATH |
| 不识别 `--rollback-on-failure` | Helm 是否为约定的 4.x 版本 |
| 资源已存在、ownership 校验失败 | 是否属于其他 release，或尚未完成旧资源接管 |
| Helm 无权读取或创建 Secret | kubeconfig 是否具备目标命名空间的 release 存储权限 |
| Secret 不存在 | 所选环境、命名空间、Secret 名称与数据键 |
| 推送失败 / 拉取失败 | 分别检查 Jenkins 推送凭据、TKE 拉取权限与镜像地址 |
| Pod Pending / 资源不足 | 节点剩余容量、requests、滚动更新额外副本 |
| 容器反复重启 | 上次退出日志、配置、外部依赖、内存与探针路径 |
| 域名 504 | CLB 后端健康、NodePort、节点安全组和 Service 端点 |
| Helm 超时 | 工作负载事件、探针、拉取状态及实际回退结果 |
| 集群访问超时 | kubeconfig、context、API Server 地址和网络可达性 |

容器重启时，在控制台查到实际 Pod 名，再执行以下命令；不要把带敏感内容的日志复制到公共位置：

```bash
"${kube[@]}" describe pod POD_NAME
"${kube[@]}" logs POD_NAME -c sample-service --previous --tail=100
```

TKE 配套说明：[共享 CLB](https://cloud.tencent.com.cn/document/product/457/127545)、[镜像免密拉取](https://cloud.tencent.com/document/product/457/49225)、[网络排障](https://cloud.tencent.com/document/product/457/80913)。
