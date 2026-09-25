#!/usr/bin/env bash
# 离线执行真实发布脚本和 Helm；Docker、集群请求和 HTTP 使用命令替身。
set -euo pipefail
source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HELM_REAL="$(command -v helm)"
export HELM_REAL
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT
mkdir -p "$test_root/deploy/jenkins" "$test_root/bin"
cp "$source_root/deploy/jenkins/release.sh" "$test_root/deploy/jenkins/"
cp -R "$source_root/deploy/helm" "$test_root/deploy/"
cd "$test_root"
git init --quiet
git -c user.name='Release Test' -c user.email=test@example.invalid -c commit.gpgsign=false \
    commit --allow-empty --quiet -m fixture
export COMMAND_LOG="$test_root/commands.jsonl"
export AUTH_PATH="$test_root/auth-path"
export KUBECONFIG="$test_root/kubeconfig" KUBE_CONTEXT=test-context
printf '# offline fixture\n' > "$KUBECONFIG"
export API_REPOSITORY=example.invalid/api WEB_REPOSITORY=example.invalid/web
export REGISTRY_HOST=example.invalid BUILD_NUMBER=42 IMAGE_PLATFORM=linux/amd64
export TCR_USERNAME=fixture TCR_PASSWORD=fixture-password IMAGE_PULL_SECRET=tcr-pull
export DEPLOY_ENV=test CASE=success RUN_QUALITY_CHECKS=false
export PATH="$test_root/bin:$PATH"

cat > bin/mock <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
tool="${0##*/}"
jq -cn --args '$ARGS.positional' -- "$tool" "$@" >> "$COMMAND_LOG"
args=" $* "
case "$tool" in
    docker)
        case "$1 $2" in
            'login '* )
                cat >/dev/null
                printf '%s' "$DOCKER_CONFIG" > "$AUTH_PATH"
                [[ "$CASE" != login-failure ]] || exit 1
                if [[ "$CASE" == base-login-failure && "$2" == fushangyun-vpc.tencentcloudcr.com ]]; then exit 1; fi
                ;;
            'buildx build')
                [[ "$CASE" != build-failure ]] || exit 1
                while [[ "$1" != --metadata-file ]]; do shift; done
                file="$2"
                digest="$(printf '%064d' 1)"
                [[ "$file" != *web-build.json ]] || digest="$(printf '%064d' 2)"
                if [[ "$CASE" == missing-digest ]]; then
                    printf '{}\n' > "$file"
                else
                    [[ "$CASE" != invalid-digest ]] || digest=invalid
                    jq -n --arg digest "sha256:$digest" '{"containerimage.digest":$digest}' > "$file"
                fi
                ;;
        esac
        ;;
    helm)
        if [[ "$1" != --kubeconfig ]]; then
            if [[ "$CASE" == helm3 && "$1" == version ]]; then printf v3.19.0; exit; fi
            exec "$HELM_REAL" "$@"
        fi
        if [[ "$args" == *' --dry-run=server '* ]]; then
            [[ "$CASE" != ownership-failure ]] || exit 1
        elif [[ "$args" == *' upgrade '* ]]; then
            [[ "$CASE" != upgrade-failure ]] || exit 1
        elif [[ "$args" == *' rollback '* ]]; then
            [[ "$CASE" != rollback-failure ]] || exit 1
        elif [[ "$args" == *' history '* ]]; then
            case "$CASE" in
                rollback-current)
                    printf '%s\n' '[{"revision":2,"status":"deployed"}]'
                    ;;
                rollback-missing)
                    printf '%s\n' '[{"revision":3,"status":"deployed"}]'
                    ;;
                rollback-failed-status)
                    printf '%s\n' '[{"revision":2,"status":"failed"},{"revision":3,"status":"deployed"}]'
                    ;;
                rollback-ok|rollback-failure|rollback-bad-image|rollback-image-mismatch|rollback-rollout-failure|rollback-http-failure)
                    printf '%s\n' '[{"revision":2,"status":"superseded"},{"revision":3,"status":"deployed"}]'
                    ;;
                *)
                    printf '[]\n'
                    ;;
            esac
            exit 0
        elif [[ "$args" == *' get values '* ]]; then
            if [[ "$CASE" == rollback-bad-image ]]; then
                printf '%s\n' '{"api":{"image":"latest"},"client":{"image":"latest"}}'
            else
                jq -n --arg api "example.invalid/api@sha256:$(printf 'a%.0s' {1..64})" \
                    --arg web "example.invalid/web@sha256:$(printf 'b%.0s' {1..64})" \
                    '{api:{image:$api},client:{image:$web}}'
            fi
            exit 0
        fi
        printf '[]\n'
        ;;
    kubectl)
        if [[ "$args" == *' current-context '* ]]; then
            [[ "$CASE" != missing-context ]] || exit 1
            printf 'uploaded-context\n'
        elif [[ "$args" == *' get ingress '* ]]; then
            [[ "$CASE" != ingress-read-failure ]] || exit 1
            [[ "$CASE" != new-ingress ]] || exit 0
            group=true; clb=lb-gpk8k2ps
            [[ "$CASE" != legacy-ingress ]] || group=false
            [[ "$CASE" != wrong-clb ]] || clb=lb-other
            jq -n --arg group "$group" --arg clb "$clb" '{metadata:{annotations:{
                "ingress.cloud.tencent.com/enable-group":$group,"kubernetes.io/ingress.existLbId":$clb}}}'
        elif [[ "$args" == *' get secret '* ]]; then
            [[ "$CASE" != missing-secret ]] || exit 1
            printf present
        elif [[ "$args" == *' --dry-run=server '* ]]; then
            [[ "$CASE" != dry-run-failure ]]
        elif [[ "$args" == *' rollout '* ]]; then
            [[ "$CASE" != rollout-failure && "$CASE" != rollback-rollout-failure ]]
        elif [[ "$args" == *' get deployments '* ]]; then
            jq --arg case "$CASE" '{items:[
                {metadata:{name:"erp-api"},spec:{template:{spec:{containers:[{name:"erp-api",image:.api.image}]}}}},
                {metadata:{name:"erp-client"},spec:{template:{spec:{containers:[{name:"erp-client",image:.client.image}]}}}}
            ]} | if $case == "image-mismatch" or $case == "rollback-image-mismatch" then .items[0].spec.template.spec.containers[0].image = "wrong"
                elif $case == "missing-deployment" then .items = [.items[0]] else . end' release-artifacts/values-release.json
        fi
        ;;
    curl) [[ "$CASE" != http-failure && "$CASE" != rollback-http-failure ]] ;;
esac
MOCK
chmod +x bin/mock
for tool in docker helm kubectl curl; do ln -s mock "bin/$tool"; done

checks=0
check_log() { jq -es "$1" "$COMMAND_LOG" >/dev/null; }
run_ok() {
    : > "$COMMAND_LOG"
    if ! bash deploy/jenkins/release.sh "$1" > output.log 2>&1; then cat output.log; exit 1; fi
    checks=$((checks + 1))
}
run_fail() {
    : > "$COMMAND_LOG"
    if bash deploy/jenkins/release.sh "$1" > output.log 2>&1; then
        echo "应拒绝: $1 / $CASE / $DEPLOY_ENV" >&2; exit 1
    fi
    checks=$((checks + 1))
}
no_upgrade() { check_log 'all(.[]; index("upgrade") == null)'; }
no_live_upgrade() { check_log 'all(.[]; index("upgrade") == null or index("--dry-run=server") != null)'; }

# 两个环境都执行真实 Helm lint/package/template，并确认构建地址、digest 和部署隔离。
run_ok validate
for DEPLOY_ENV in test production; do
    export DEPLOY_ENV
    namespace=test; suffix=-test
    if [[ "$DEPLOY_ENV" == production ]]; then namespace=prod; suffix=; fi
    run_ok build
    check_log 'any(.[]; .[0:3] == ["docker", "login", "example.invalid"])'
    check_log 'any(.[]; .[0:3] == ["docker", "login", "fushangyun-vpc.tencentcloudcr.com"])'
    check_log 'any(.[]; .[0:3] == ["docker", "buildx", "create"] and
        .[index("--driver-opt") + 1] == "image=fushangyun-vpc.tencentcloudcr.com/base/buildkit@sha256:28a898719c18a33f4e8000685287fa36fd0dd9560c6440227d3a732d79bb41d8")'
    jq -e --arg env "$DEPLOY_ENV" --arg ns "$namespace" \
        '.environment == $env and .namespace == $ns and (.api.image | endswith("0001"))' \
        release-artifacts/values-release.json >/dev/null
    grep -q "namespace: $namespace" release-artifacts/manifests.yaml
    [[ "$(grep -c '^kind:' release-artifacts/manifests.yaml)" == 10 ]]
    if grep -Eq '^kind: (Secret|Namespace)$|defaultServer:' release-artifacts/manifests.yaml; then exit 1; fi
    check_log "any(.[]; index(\"NEXT_PUBLIC_API_BASE_URL=https://erp-api$suffix.fushangyunfu.com\") != null)"
    [[ ! -d "$(cat "$AUTH_PATH")" ]]
    run_ok deploy
    test -s release-artifacts/deployment-status.txt
    check_log "all(.[]; if (.[0] == \"kubectl\" or (.[0] == \"helm\" and index(\"upgrade\") != null))
        then .[index(\"--namespace\") + 1] == \"$namespace\" else true end)"
    check_log 'any(.[]; index("--rollback-on-failure") != null and index("--wait") != null)'
    check_log 'all(.[]; index("--take-ownership") == null and (index("apply") == null or index("--dry-run=server") != null))'
    check_log "any(.[]; .[0] == \"curl\" and .[-1] == \"https://erp-api$suffix.fushangyunfu.com/health\")"
done
# 生产产物不得用于测试；不调用集群。
export DEPLOY_ENV=test
run_fail deploy
check_log 'length == 0'
export DEPLOY_ENV=production
for CASE in missing-secret legacy-ingress wrong-clb ingress-read-failure dry-run-failure ownership-failure \
    upgrade-failure rollout-failure image-mismatch missing-deployment http-failure; do
    export CASE
    printf 'stale success' > release-artifacts/deployment-status.txt
    run_fail deploy
    test ! -e release-artifacts/deployment-status.txt
    case "$CASE" in
        missing-secret|legacy-ingress|wrong-clb|ingress-read-failure|dry-run-failure) no_upgrade ;;
        ownership-failure) no_live_upgrade ;;
    esac
done
export CASE=new-ingress
run_ok deploy
export CASE=success KUBE_CONTEXT=''
run_ok deploy
check_log 'any(.[]; index("uploaded-context") != null)'
export CASE=missing-context
run_fail deploy
check_log 'all(.[]; .[0] != "kubectl" or index("current-context") != null)'
export CASE=success KUBE_CONTEXT=test-context
mv "$KUBECONFIG" "$KUBECONFIG.saved"
run_fail deploy
check_log 'all(.[]; .[0] != "kubectl" and index("upgrade") == null)'
mv "$KUBECONFIG.saved" "$KUBECONFIG"
for DEPLOY_ENV in '' invalid ../production; do
    export DEPLOY_ENV
    run_fail preflight
    check_log 'length == 0'
done
export DEPLOY_ENV=production CASE=helm3
run_fail validate
export CASE=success
# Chart 自身拒绝不匹配的命名空间，即使绕过发布脚本。
if "$HELM_REAL" template erp release-artifacts/chart.tgz -n test -f release-artifacts/values-release.json > output.log 2>&1; then exit 1; fi
checks=$((checks + 1))
for CASE in login-failure base-login-failure build-failure invalid-digest missing-digest; do
    export CASE
    run_fail build
    test ! -e release-artifacts/chart.tgz
    [[ ! -d "$(cat "$AUTH_PATH")" ]]
    if grep -q "$TCR_PASSWORD" output.log; then echo '凭据出现在日志中' >&2; exit 1; fi
    if [[ "$CASE" == login-failure || "$CASE" == base-login-failure ]]; then
        check_log 'all(.[]; index("buildx") == null)'
    else
        check_log 'any(.[]; .[0:3] == ["docker", "buildx", "rm"])'
    fi
done
export CASE=success IMAGE_PULL_SECRET=''
run_ok build
if grep -q imagePullSecrets release-artifacts/manifests.yaml; then exit 1; fi
export IMAGE_PULL_SECRET='invalid/name'
run_fail build
test ! -e release-artifacts/chart.tgz
# 回退只接受已被替换的历史 revision，并核对目标镜像；不构建、不升级。
export IMAGE_PULL_SECRET=tcr-pull DEPLOY_ENV=test CASE=rollback-ok ROLLBACK_REVISION=2
no_cluster_change() { check_log 'all(.[]; index("rollback") == null and index("upgrade") == null and .[0] != "kubectl")'; }
for ROLLBACK_REVISION in '' 0 -1 01 abc; do
    export ROLLBACK_REVISION
    run_fail rollback
    no_cluster_change
done
export ROLLBACK_REVISION=2
for CASE in rollback-current rollback-missing rollback-failed-status rollback-bad-image; do
    export CASE
    printf 'stale success' > release-artifacts/deployment-status.txt
    run_fail rollback
    test ! -e release-artifacts/deployment-status.txt
    check_log 'all(.[]; index("rollback") == null and index("upgrade") == null)'
done
export CASE=rollback-failure
printf 'stale success' > release-artifacts/deployment-status.txt
run_fail rollback
test ! -e release-artifacts/deployment-status.txt
check_log 'any(.[]; index("rollback") != null and index("--history-max") != null)'
for CASE in rollback-image-mismatch rollback-rollout-failure rollback-http-failure; do
    export CASE
    printf 'stale success' > release-artifacts/deployment-status.txt
    run_fail rollback
    test ! -e release-artifacts/deployment-status.txt
done
for DEPLOY_ENV in test production; do
    export DEPLOY_ENV CASE=rollback-ok ROLLBACK_REVISION=2
    namespace=test
    [[ "$DEPLOY_ENV" == production ]] && namespace=prod
    run_ok rollback
    grep -q '回退完成' release-artifacts/deployment-status.txt
    check_log "any(.[]; index(\"rollback\") != null and index(\"$ROLLBACK_REVISION\") != null and index(\"--history-max\") != null and index(\"20\") != null and index(\"--wait\") != null and index(\"--timeout\") != null and index(\"15m\") != null)"
    check_log 'all(.[]; index("upgrade") == null and .[0] != "docker")'
    check_log "all(.[]; if .[0] == \"kubectl\" or .[0] == \"helm\" then index(\"--namespace\") != null and .[index(\"--namespace\") + 1] == \"$namespace\" else true end)"
done
mv "$KUBECONFIG" "$KUBECONFIG.saved"
export DEPLOY_ENV=test CASE=rollback-ok
run_fail rollback
no_cluster_change
mv "$KUBECONFIG.saved" "$KUBECONFIG"
unset ROLLBACK_REVISION
# 环境文件不可复用另一个环境的域名。
jq '.ingress.apiHost = "erp-api.fushangyunfu.com"' deploy/helm/erp/environments/test.json > changed.json
mv changed.json deploy/helm/erp/environments/test.json
run_fail preflight
check_log 'length == 0'
printf '通过 %s 项离线场景（真实 Helm，模拟 Docker/集群/HTTP）。\n' "$checks"
