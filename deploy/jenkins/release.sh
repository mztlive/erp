#!/usr/bin/env bash
# 从仓库根执行；数据库与运行配置由集群外部管理。
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
chart=deploy/helm/erp
artifacts=release-artifacts
base_registry_host=fushangyun-vpc.tencentcloudcr.com
buildkit_image="$base_registry_host/base/buildkit@sha256:28a898719c18a33f4e8000685287fa36fd0dd9560c6440227d3a732d79bb41d8"

load_environment() {
    case "${DEPLOY_ENV:-}" in
        test) namespace=test ;;
        production) namespace=prod ;;
        *) echo 'DEPLOY_ENV 必须显式设置为 test 或 production。' >&2; return 1 ;;
    esac
    environment_values="$chart/environments/$DEPLOY_ENV.json"
    jq -e --arg environment "$DEPLOY_ENV" --arg namespace "$namespace" \
        '.environment == $environment and .namespace == $namespace' "$environment_values" >/dev/null
    # 同一 CLB 下四个入口必须不同，缺失域名同样拒绝。
    jq -es '[.[] | .ingress.apiHost, .ingress.webHost] |
        all(.[]; type == "string" and length > 0) and (unique | length == 4)' \
        "$chart/environments/test.json" "$chart/environments/production.json" >/dev/null
    api_url="https://$(jq -er '.ingress.apiHost' "$environment_values")"
    web_url="https://$(jq -er '.ingress.webHost' "$environment_values")"
}

validate() {
    local tool missing=0
    local tools=(git docker kubectl helm jq curl bash grep)
    if [[ "${RUN_QUALITY_CHECKS:-false}" == true ]]; then
        tools+=(cargo node npm)
    else
        echo '主机前后端质量检查已跳过；镜像构建和发布检查仍执行。'
    fi
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" >/dev/null; then
            printf '[缺失] %s：请检查 selfhost Agent 的工具与 PATH。\n' "$tool" >&2
            missing=1
        fi
    done
    [[ "$missing" == 0 ]] || return 1
    if [[ "$(helm version --template '{{.Version}}')" != v4.* ]]; then
        echo '发布要求 Helm 4.x；请固定经过验证的补丁版本。' >&2
        return 1
    fi
    load_environment
    docker buildx version
    docker info --format '{{.ServerVersion}}'
    git diff --check
    bash -n deploy/jenkins/release.sh
}

preflight() {
    load_environment
    local context="${KUBE_CONTEXT:-}" ingress clb config_secret tls_secret
    if [[ ! -f "${KUBECONFIG:-}" ]]; then
        echo '缺少 Jenkins 上传的 kubeconfig 文件，不使用 Agent 默认配置。' >&2
        return 1
    fi
    if [[ -z "$context" ]]; then
        context="$(kubectl --kubeconfig "$KUBECONFIG" config current-context)"
        [[ -n "$context" ]] || { echo '上传的 kubeconfig 未设置 current-context。' >&2; return 1; }
    fi
    kube=(kubectl --kubeconfig "$KUBECONFIG" --context "$context" --namespace "$namespace" --request-timeout=30s)
    helm_target=(helm --kubeconfig "$KUBECONFIG" --kube-context "$context" --namespace "$namespace")
    clb="$(jq -er '.ingress.clbId' "$environment_values")"
    config_secret="$(jq -er '.api.configSecret' "$environment_values")"
    tls_secret="$(jq -er '.ingress.tlsSecret' "$environment_values")"
    ingress="$("${kube[@]}" get ingress erp --ignore-not-found -o json)"
    if [[ -n "$ingress" ]] && ! jq -e --arg clb "$clb" '
        .metadata.annotations | .["ingress.cloud.tencent.com/enable-group"] == "true"
        and .["kubernetes.io/ingress.existLbId"] == $clb' <<< "$ingress" >/dev/null; then
        echo '现有 erp Ingress 未启用共享或绑定其他 CLB；必须先完成入口迁移。' >&2
        return 1
    fi
    # 只校验数据键，不输出 Secret 内容。
    "${kube[@]}" get secret "$config_secret" -o go-template='{{if index .data "config.toml"}}present{{end}}' | grep -qx present
    "${kube[@]}" get secret "$tls_secret" -o go-template='{{if index .data "qcloud_cert_id"}}present{{end}}' | grep -qx present
    if [[ -n "${IMAGE_PULL_SECRET:-}" ]]; then
        "${kube[@]}" get secret "$IMAGE_PULL_SECRET" \
            -o go-template='{{if index .data ".dockerconfigjson"}}present{{end}}' | grep -qx present
    fi
}

build() (
    load_environment
    : "${TCR_USERNAME:?缺少 TCR 用户名}" "${TCR_PASSWORD:?缺少 TCR 密码}" "${BUILD_NUMBER:?缺少构建编号}"
    # 构建与打包在同一环境完成，不保留上次运行的发布包或成功标记。
    mkdir -p "$artifacts"
    rm -f "$artifacts"/{chart.tgz,values-release.json,manifests.yaml,deployment-status.txt,helm-history.json,deployments.json,api-build.json,web-build.json}
    docker_auth_dir="$(mktemp -d)"
    export DOCKER_CONFIG="$docker_auth_dir"
    builder_name="erp-$(date +%s)-$$"
    builder_started=false
    cleanup_build() {
        build_status=$?
        trap - EXIT
        if [[ "$builder_started" == true ]]; then
            docker buildx rm "$builder_name" >/dev/null 2>&1 || true
        fi
        rm -rf "$docker_auth_dir"
        exit "$build_status"
    }
    trap cleanup_build EXIT
    # Docker 按域名保存凭据；成品推送与内网基础镜像拉取均须登录。
    for registry in "$REGISTRY_HOST" "$base_registry_host"; do
        if ! printf '%s' "$TCR_PASSWORD" | docker login "$registry" --username "$TCR_USERNAME" --password-stdin; then
            printf 'TCR 登录失败（%s）；请检查 Jenkins 凭据、域名解析与实例访问策略。\n' "$registry" >&2
            exit 1
        fi
    done
    builder_started=true
    docker buildx create --name "$builder_name" --driver docker-container \
        --driver-opt "image=$buildkit_image" --use >/dev/null
    commit="$(git rev-parse HEAD)"
    tag="${commit:0:12}-${DEPLOY_ENV}-${BUILD_NUMBER}"
    docker buildx build --builder "$builder_name" --platform "$IMAGE_PLATFORM" \
        --file deploy/jenkins/Dockerfile.web-api --tag "$API_REPOSITORY:$tag" --push \
        --metadata-file "$artifacts/api-build.json" backend
    docker buildx build --builder "$builder_name" --platform "$IMAGE_PLATFORM" \
        --file erp-client/Dockerfile --build-arg "NEXT_PUBLIC_API_BASE_URL=$api_url" \
        --tag "$WEB_REPOSITORY:$tag" --push --metadata-file "$artifacts/web-build.json" erp-client
    api_digest="$(jq -er '."containerimage.digest" | strings | select(test("^sha256:[0-9a-f]{64}$"))' "$artifacts/api-build.json")"
    web_digest="$(jq -er '."containerimage.digest" | strings | select(test("^sha256:[0-9a-f]{64}$"))' "$artifacts/web-build.json")"
    jq --arg api "$API_REPOSITORY@$api_digest" --arg web "$WEB_REPOSITORY@$web_digest" \
        --arg release "$commit-$DEPLOY_ENV-$BUILD_NUMBER" --arg pull "${IMAGE_PULL_SECRET:-}" \
        '.api.image = $api | .client.image = $web | .releaseId = $release | .imagePullSecret = $pull' \
        "$environment_values" > "$artifacts/values-release.json"
    helm lint "$chart" --strict --namespace "$namespace" -f "$artifacts/values-release.json"
    helm package "$chart" --destination "$docker_auth_dir"
    mv "$docker_auth_dir"/erp-*.tgz "$artifacts/chart.tgz"
    helm template erp "$artifacts/chart.tgz" --namespace "$namespace" -f "$artifacts/values-release.json" \
        > "$artifacts/manifests.yaml"
)

deploy() {
    rm -f "$artifacts/deployment-status.txt"
    load_environment
    # 校验目标环境；渲染时由 Chart schema 继续校验镜像 digest 和其余参数。
    jq -e --slurpfile environment "$environment_values" --arg pull "${IMAGE_PULL_SECRET:-}" '
        [.environment, .namespace, .ingress, .api.configSecret, .imagePullSecret] ==
        [$environment[0].environment, $environment[0].namespace, $environment[0].ingress,
         $environment[0].api.configSecret, $pull]' "$artifacts/values-release.json" >/dev/null
    helm template erp "$artifacts/chart.tgz" --namespace "$namespace" -f "$artifacts/values-release.json" \
        > "$artifacts/manifests.yaml"
    preflight
    "${kube[@]}" apply --dry-run=server -f "$artifacts/manifests.yaml" >/dev/null
    "${helm_target[@]}" upgrade --install erp "$artifacts/chart.tgz" \
        -f "$artifacts/values-release.json" --reset-values --dry-run=server --hide-secret >/dev/null
    "${helm_target[@]}" upgrade --install erp "$artifacts/chart.tgz" \
        -f "$artifacts/values-release.json" --reset-values \
        --wait --rollback-on-failure --timeout 15m --history-max 20
    "${helm_target[@]}" history erp -o json > "$artifacts/helm-history.json"
    "${kube[@]}" rollout status deployment/erp-api --timeout=900s
    "${kube[@]}" rollout status deployment/erp-client --timeout=900s
    "${kube[@]}" get deployments erp-api erp-client -o json > "$artifacts/deployments.json"
    jq -e --slurpfile values "$artifacts/values-release.json" '
        [.items[] | .metadata.name as $name | .spec.template.spec.containers[] |
         select(.name == $name) | {key: $name, value: .image}] | from_entries ==
        {"erp-api": $values[0].api.image, "erp-client": $values[0].client.image}' \
        "$artifacts/deployments.json" >/dev/null
    curl --fail --silent --show-error --retry 12 --retry-all-errors --retry-delay 10 \
        --connect-timeout 10 --max-time 20 --output /dev/null "$api_url/health"
    curl --fail --silent --show-error --retry 12 --retry-all-errors --retry-delay 10 \
        --connect-timeout 10 --max-time 20 --output /dev/null "$web_url/"
    printf 'rollout、镜像核对与公网 HTTP 检查通过\n' > "$artifacts/deployment-status.txt"
}

case "${1:-}" in
    validate) validate ;;
    preflight) preflight ;;
    build) build ;;
    deploy) deploy ;;
    *) echo '用法: bash deploy/jenkins/release.sh validate|preflight|build|deploy' >&2; exit 2 ;;
esac
