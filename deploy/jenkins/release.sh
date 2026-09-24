#!/usr/bin/env bash
# 从仓库根执行；运行时配置和数据库由集群外部管理。
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

kube() {
    kubectl --context "${KUBE_CONTEXT:?必须指定 KUBE_CONTEXT}" \
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
    kube get secret web-api-config -o go-template='{{if index .data "config.toml"}}present{{end}}' | grep -qx present
    kube get secret fsytsl-wsk87cm7 -o go-template='{{if index .data "qcloud_cert_id"}}present{{end}}' | grep -qx present
    if [[ -n "${IMAGE_PULL_SECRET:-}" ]]; then
        kube get secret "$IMAGE_PULL_SECRET" \
            -o go-template='{{if index .data ".dockerconfigjson"}}present{{end}}' | grep -qx present
    fi
}

build() {
    : "${TCR_USERNAME:?缺少 TCR 用户名}" "${TCR_PASSWORD:?缺少 TCR 密码}"
    local docker_auth_dir builder_name tag
    docker_auth_dir="$(mktemp -d)"
    export DOCKER_CONFIG="$docker_auth_dir"
    # 独立的 buildx builder 与凭据目录，不修改同机其他任务的 Docker 登录状态。
    builder_name="erp-$(date +%s)-$$"
    trap 'docker buildx rm "$builder_name" >/dev/null 2>&1 || true; rm -rf "$docker_auth_dir"' EXIT
    printf '%s' "$TCR_PASSWORD" | docker login "$REGISTRY_HOST" --username "$TCR_USERNAME" --password-stdin
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
    # EXIT trap 使用函数局部变量，必须在函数返回前执行清理。
    docker buildx rm "$builder_name" >/dev/null
    rm -rf "$docker_auth_dir"
    trap - EXIT
}

deploy() {
    preflight
    # 先让 API Server 验证所有对象；失败时不执行正式 apply。
    kube apply --dry-run=server -f release-artifacts/manifests.yaml >/dev/null
    kube apply -f release-artifacts/manifests.yaml
    kube rollout status deployment/web-api --timeout=900s
    kube rollout status deployment/erp-client --timeout=900s
    kube get deployments web-api erp-client -o json > release-artifacts/deployments.json
    python3 - <<'PY'
import json
from pathlib import Path
root = Path('release-artifacts')
release = json.loads((root / 'release.json').read_text())
expected = {'web-api': release['api_image'], 'erp-client': release['web_image']}
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
    validate)
        for tool in git docker kubectl python3 curl cargo npm; do command -v "$tool" >/dev/null; done
        docker buildx version
        git diff --check
        bash -n deploy/jenkins/release.sh
        ;;
    preflight) preflight ;;
    build) build ;;
    deploy) deploy ;;
    *) echo '用法: bash deploy/jenkins/release.sh validate|preflight|build|deploy' >&2; exit 2 ;;
esac
