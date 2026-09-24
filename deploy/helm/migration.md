# ERP 现有 Kubernetes 资源 Helm 接管执行规范

## 1. 适用条件

本流程只用于已经通过 Kustomize/kubectl 部署、尚未归属任何 Helm release 的 ERP 资源。空的测试命名空间按正常 Jenkins 发布，不执行本流程。存在其他 Helm release 归属时必须停止，禁止覆盖其归属。

接管只改变 ERP 资源的管理方式。保留当前成功版本的镜像 digest、资源名称、Deployment selector、Service NodePort 和共享 CLB 入口；不得同时执行数据库变更或切换 Ingress 模式。Pod 注解变化可能触发滚动更新，必须预留容量和维护窗口。

## 2. 接管前核对

1. 暂停旧应用发布入口及日常 Jenkins 发布，确保没有其他任务操作同一环境。
2. 确认 kubeconfig、context、命名空间和 ERP 归属。生产选择 `production/prod`；后续命令在仓库根和同一 Bash 会话执行。
3. 执行 `helm history erp`，确认目标 namespace 尚无 release；网络、认证、权限错误不得当作无 release。
4. 导出现有 ERP 资源，检查每个对象的 `app.kubernetes.io/part-of`、名称、镜像、selector、Service 端口、Ingress 域名和 CLB 注解。若存在 `meta.helm.sh/release-name` 或其他工具仍在协调资源，必须停止并先解决归属。
5. 确认 API 和前端已经使用 `erp-api`、`erp-client`，且均为可回退的 digest 镜像；旧 `web-api` 名称的切换必须先独立完成。

```bash
set -euo pipefail
export DEPLOY_ENV=production
export KUBECONFIG=/path/to/kubeconfig
export KUBE_CONTEXT=目标context
# 若使用 TKE 免密拉取，保持为空；否则填当前工作负载使用的 Secret。
export IMAGE_PULL_SECRET=''
exports="$(python3 deploy/jenkins/environment.py)"
eval "$exports"
mkdir -p release-artifacts/migration
bash deploy/jenkins/release.sh preflight
kubectl --kubeconfig "$KUBECONFIG" --context "$KUBE_CONTEXT" -n "$KUBE_NAMESPACE" get \
  deployment/erp-api deployment/erp-client service/erp-api service/erp-client \
  serviceaccount/erp-api serviceaccount/erp-client poddisruptionbudget/erp-api poddisruptionbudget/erp-client \
  ingress/erp tkeserviceconfig/erp -o yaml > release-artifacts/migration/before.yaml
```

必须备份这份清单到受控位置。此备份不含应用 Secret；已有 Secret 由配置管理方负责保留。缺少预期资源时先核实差异，不得把失败的导出当成完整备份。

## 3. 使用现有镜像建立基线

读取线上两组镜像，验证均为 `repository@sha256:...`。环境 values 的副本数、资源额度、PDB 和入口必须先调整到已核对的线上配置。Chart 固定保留旧 selector，禁止加入 release 标签改变 Deployment selector。

```bash
api_image="$(kubectl --kubeconfig "$KUBECONFIG" --context "$KUBE_CONTEXT" -n "$KUBE_NAMESPACE" get deployment erp-api -o jsonpath='{.spec.template.spec.containers[?(@.name=="erp-api")].image}')"
web_image="$(kubectl --kubeconfig "$KUBECONFIG" --context "$KUBE_CONTEXT" -n "$KUBE_NAMESPACE" get deployment erp-client -o jsonpath='{.spec.template.spec.containers[?(@.name=="erp-client")].image}')"
helm template erp deploy/helm/erp -n "$KUBE_NAMESPACE" \
  -f "deploy/helm/erp/environments/$DEPLOY_ENV.json" \
  --set-string "api.image=$api_image" --set-string "client.image=$web_image" \
  --set-string "imagePullSecret=$IMAGE_PULL_SECRET" \
  > release-artifacts/migration/helm-baseline.yaml
```

对照 `before.yaml` 和 `helm-baseline.yaml` 审查全部资源差异，允许新增 Helm 元数据和 Pod 发布/配置版本注解；不得未经核对改变入口、服务端口、探针、安全配置、资源额度或副本策略。Helm 渲染结果未指定 Service 已分配的 NodePort，不得主动删除或重建 Service。

归属和差异核对通过后，执行一次性接管。先 dry-run，再执行同参数正式命令。`--take-ownership` 仅允许用于本节确认属于 ERP 的 10 个资源。

```bash
helm --kubeconfig "$KUBECONFIG" --kube-context "$KUBE_CONTEXT" -n "$KUBE_NAMESPACE" \
  upgrade --install erp deploy/helm/erp \
  -f "deploy/helm/erp/environments/$DEPLOY_ENV.json" \
  --set-string "api.image=$api_image" --set-string "client.image=$web_image" \
  --set-string "imagePullSecret=$IMAGE_PULL_SECRET" \
  --take-ownership --dry-run=server --hide-secret

helm --kubeconfig "$KUBECONFIG" --kube-context "$KUBE_CONTEXT" -n "$KUBE_NAMESPACE" \
  upgrade --install erp deploy/helm/erp \
  -f "deploy/helm/erp/environments/$DEPLOY_ENV.json" \
  --set-string "api.image=$api_image" --set-string "client.image=$web_image" \
  --set-string "imagePullSecret=$IMAGE_PULL_SECRET" \
  --take-ownership --wait --timeout 15m --history-max 20
```

首次接管不得附加 `--rollback-on-failure`，避免首次安装失败时卸载已接管的既有业务资源。失败时保留现场，修正差异后重试基线；不得通过 `helm uninstall` 清除 release 后重来。

## 4. 验收与交接

必须确认两个 Deployment rollout 成功，镜像保持接管前 digest，Service NodePort 未变化，Ingress 仍绑定原共享 CLB，两个公网 HTTPS 入口可达，并完成关键业务验证。

通过后归档 `helm get values erp -a`、`helm get manifest erp`、`helm history erp` 与接管前备份，记录基线 revision。以上命令均须显式携带同一 kubeconfig/context/namespace。停止旧的应用 Kustomize 发布入口，再允许 Jenkins 执行普通升级。

基线 release 成功建立后，后续升级才具备 Helm 可回退的上一版本。回退按 [Jenkins 发布执行规范](../jenkins/README.md)执行；Helm 回退不会恢复配置 Secret 或数据库。
