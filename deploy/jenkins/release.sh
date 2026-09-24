#!/usr/bin/env bash
# 从仓库根执行；运行时配置和数据库由集群外部管理。
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

validate() {
    local tool location missing=0
    local tools=(git docker kubectl python3 curl bash grep)
    if [[ "${RUN_QUALITY_CHECKS:-false}" == "true" ]]; then
        tools+=(cargo node npm)
    else
        echo '质量检查已跳过；不检查 Agent 上的 Cargo、Node.js 和 npm。镜像编译仍在 Docker 中执行。'
    fi
    echo '检查 selfhost Agent 的命令环境：'
    for tool in "${tools[@]}"; do
        if location="$(command -v "$tool")"; then
            printf '[OK] %s: %s\n' "$tool" "$location"
        else
            printf '[缺失] %s：Agent 的 PATH 中找不到该命令。\n' "$tool" >&2
            missing=1
        fi
    done
    if [[ "$missing" -ne 0 ]]; then
        echo '工具预检查失败。请在 selfhost 上安装缺失工具，或为 Jenkins Agent 服务配置 PATH；SSH 终端可用不代表 Agent 可用。' >&2
        return 1
    fi
    echo '检查 Docker Buildx：'
    if ! docker buildx version; then
        echo 'Docker Buildx 不可用，请为 Agent 安装可访问的 Buildx 插件。' >&2
        return 1
    fi
    echo '检查 Docker daemon 访问权限：'
    if ! docker info --format '{{.ServerVersion}}'; then
        echo 'Agent 无法访问 Docker daemon，请检查 Docker 服务、连接配置及 Agent 运行用户的权限。' >&2
        return 1
    fi
    echo '检查发布脚本语法与 Git 差异：'
    git diff --check
    bash -n deploy/jenkins/release.sh
    echo '工具预检查通过。'
}

kube() {
    local selected_context="${KUBE_CONTEXT:-}"
    # 只读取 Jenkins 上传的文件，不回退到 Agent 自己的 ~/.kube/config。
    if [[ ! -f "${KUBECONFIG:-}" ]]; then
        echo '缺少 Jenkins 上传的 kubeconfig 文件，请检查集群凭据。' >&2
        return 1
    fi
    if [[ -z "$selected_context" ]]; then
        if ! selected_context="$(kubectl --kubeconfig "$KUBECONFIG" config current-context)" || [[ -z "$selected_context" ]]; then
            echo '上传的 kubeconfig 未设置 current-context；请设置文件默认 context，或填写可选参数 KUBE_CONTEXT。' >&2
            return 1
        fi
    fi
    kubectl --kubeconfig "$KUBECONFIG" --context "$selected_context" \
        --namespace "${KUBE_NAMESPACE:-prod}" --request-timeout=30s "$@"
}

preflight() {
    local existing_ingress
    # 不存在时允许首次创建；权限/网络错误必须停止，不能当作不存在。
    existing_ingress="$(kube get ingress erp --ignore-not-found -o json)"
    printf '%s' "$existing_ingress" | python3 -c '
import json, sys
raw = sys.stdin.read().strip()
if raw:
    annotations = json.loads(raw).get("metadata", {}).get("annotations", {})
    if (annotations.get("ingress.cloud.tencent.com/enable-group") != "true"
            or annotations.get("kubernetes.io/ingress.existLbId") != "lb-gpk8k2ps"):
        raise SystemExit("现有 erp Ingress 未启用共享或绑定了其他 CLB；请先规划迁移，流水线不会删除或原地切换入口。")
'
    # 仅验证数据键存在，不打印配置或证书内容。
    kube get secret erp-api-config -o go-template='{{if index .data "config.toml"}}present{{end}}' | grep -qx present
    kube get secret fsytsl-wsk87cm7 -o go-template='{{if index .data "qcloud_cert_id"}}present{{end}}' | grep -qx present
    if [[ -n "${IMAGE_PULL_SECRET:-}" ]]; then
        kube get secret "$IMAGE_PULL_SECRET" \
            -o go-template='{{if index .data ".dockerconfigjson"}}present{{end}}' | grep -qx present
    fi
}

build() (
    : "${TCR_USERNAME:?缺少 TCR 用户名}" "${TCR_PASSWORD:?缺少 TCR 密码}"
    # 子 shell 内的变量在 EXIT 清理期间保持可见，且不修改调用方环境。
    builder_started=false
    docker_auth_dir="$(mktemp -d)"
    export DOCKER_CONFIG="$docker_auth_dir"
    # 独立的 buildx builder 与凭据目录，不修改同机其他任务的 Docker 登录状态。
    builder_name="erp-$(date +%s)-$$"
    cleanup_build() {
        build_status=$?
        trap - EXIT
        if [[ "$builder_started" == "true" ]]; then
            docker buildx rm "$builder_name" >/dev/null 2>&1 || true
        fi
        rm -rf "$docker_auth_dir" || true
        exit "$build_status"
    }
    trap cleanup_build EXIT
    if printf '%s' "$TCR_PASSWORD" | docker login "$REGISTRY_HOST" --username "$TCR_USERNAME" --password-stdin; then
        echo 'TCR 登录成功。'
    else
        login_status=$?
        echo 'TCR 登录失败：请检查 Jenkins 的 TCR_CREDENTIALS_ID 所指用户名/密码是否属于目标实例、是否有效，以及实例网络访问策略。TKE 免密拉取不提供 Jenkins 推送权限。' >&2
        exit "$login_status"
    fi
    builder_started=true
    docker buildx create --name "$builder_name" --driver docker-container --use >/dev/null
    tag="$(git rev-parse --short=12 HEAD)-${BUILD_NUMBER:?缺少构建编号}"
    mkdir -p release-artifacts
    docker buildx build --builder "$builder_name" --platform "$IMAGE_PLATFORM" \
        --file deploy/jenkins/Dockerfile.web-api \
        --tag "$API_REPOSITORY:$tag" --push \
        --metadata-file release-artifacts/api-build.json backend
    docker buildx build --builder "$builder_name" --platform "$IMAGE_PLATFORM" \
        --file erp-client/Dockerfile \
        --build-arg "NEXT_PUBLIC_API_BASE_URL=$NEXT_PUBLIC_API_BASE_URL" \
        --tag "$WEB_REPOSITORY:$tag" --push \
        --metadata-file release-artifacts/web-build.json erp-client
)

deploy() {
    preflight
    # 先让 API Server 验证所有对象；失败时不执行正式 apply。
    kube apply --dry-run=server -f release-artifacts/manifests.yaml >/dev/null
    kube apply -f release-artifacts/manifests.yaml
    kube rollout status deployment/erp-api --timeout=900s
    kube rollout status deployment/erp-client --timeout=900s
    kube get deployments erp-api erp-client -o json > release-artifacts/deployments.json
    python3 - <<'PY'
import json
from pathlib import Path
root = Path('release-artifacts')
release = json.loads((root / 'release.json').read_text())
expected = {'erp-api': release['api_image'], 'erp-client': release['web_image']}
for deployment in json.loads((root / 'deployments.json').read_text())['items']:
    name = deployment['metadata']['name']
    containers = deployment['spec']['template']['spec']['containers']
    image = next(c['image'] for c in containers if c['name'] == name)
    if image != expected[name]:
        raise SystemExit(f'{name}: 集群镜像与本次发布不一致')
PY
    # 两个域名必须解析到共享 CLB，且证书生效，公网探测才会通过。
    curl --fail --silent --show-error --retry 12 --retry-all-errors --retry-delay 10 \
        --connect-timeout 10 --max-time 20 --output /dev/null "$NEXT_PUBLIC_API_BASE_URL/health"
    curl --fail --silent --show-error --retry 12 --retry-all-errors --retry-delay 10 \
        --connect-timeout 10 --max-time 20 --output /dev/null "$WEB_URL/"
    printf 'rollout、镜像核对与公网 HTTP 检查通过\n' > release-artifacts/deployment-status.txt
}

case "${1:-}" in
    validate) validate ;;
    preflight) preflight ;;
    build) build ;;
    deploy) deploy ;;
    *) echo '用法: bash deploy/jenkins/release.sh validate|preflight|build|deploy' >&2; exit 2 ;;
esac
