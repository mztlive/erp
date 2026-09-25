# 应用发布到 TKE

按本文把应用发布到现有 Jenkins、TCR 和 TKE。下文的 `sample-service`、端口 `8080`、健康检查 `/health`、域名都换成自己的。Jenkins 凭据、Agent、共享 CLB、证书 Secret 名称保持不变。

提交到应用仓库的文件：

```text
Jenkinsfile.k8s
deploy/jenkins/release.sh
deploy/helm/sample-service/
├── Chart.yaml
├── values.yaml
├── values.schema.json
├── environments/test.json
├── environments/production.json
└── templates/
    ├── app.yaml
    ├── ingress.yaml
    └── pdb.yaml
Dockerfile
```

`release-artifacts/` 写入 `.gitignore`。

## 1. 已有环境

这些已经配好，发布脚本直接用：

| 项目 | 值 |
| --- | --- |
| Jenkins Agent | `selfhost` |
| 推送镜像凭据 | `tcr`（Username with password） |
| 集群凭据 | `tke-kubeconfig`（Secret file） |
| 镜像仓库 | `fushangyun.tencentcloudcr.com/fushangyun/<服务名>` |
| 基础镜像与 BuildKit | `fushangyun-vpc.tencentcloudcr.com/base` |
| 测试 / 生产命名空间 | `test` / `prod` |
| 共享 CLB | `lb-gpk8k2ps` |
| 证书 Secret | `fsytsl-wsk87cm7`，键 `qcloud_cert_id` |
| Helm | Agent 上已安装 4.x，当前验证版本 4.1.3 |

`DEPLOY_ENV=test` 固定进入 `test`，`production` 固定进入 `prod`。两个环境的数据库、密钥、对象存储分开，禁止把生产配置复制到测试。

应用 Chart 只管理本服务。MongoDB、Meilisearch、命名空间、证书、CLB、应用配置 Secret 不写进 Chart。

## 2. 准备 Jenkinsfile

根目录新建 `Jenkinsfile.k8s`。把两处 `sample-service` 换成自己的镜像仓库名。

```groovy
// 从仓库根运行。
pipeline {
    agent { label 'selfhost' }
    options {
        skipDefaultCheckout(true)
        disableConcurrentBuilds()
        timeout(time: 120, unit: 'MINUTES')
        buildDiscarder(logRotator(numToKeepStr: '20', artifactNumToKeepStr: '20'))
    }
    parameters {
        choice(name: 'DEPLOY_ENV', choices: ['test', 'production'], description: 'test → test；production → prod')
        string(name: 'TCR_CREDENTIALS_ID', defaultValue: 'tcr', description: 'Jenkins Username with password 凭据 ID')
        string(name: 'KUBECONFIG_CREDENTIALS_ID', defaultValue: 'tke-kubeconfig', description: 'Jenkins Secret file 凭据 ID')
        string(name: 'KUBE_CONTEXT', defaultValue: '', description: '留空使用上传 kubeconfig 的 current-context')
        string(name: 'IMAGE_PULL_SECRET', defaultValue: '', description: '已配置 TKE 免密拉取时留空')
        choice(name: 'IMAGE_PLATFORM', choices: ['linux/amd64', 'linux/arm64'], description: '必须与 TKE 节点架构一致')
        booleanParam(name: 'DEPLOY_TO_TKE', defaultValue: true, description: 'false 时只构建推送镜像并归档，不发布')
    }
    environment {
        REGISTRY_HOST = 'fushangyun.tencentcloudcr.com'
        IMAGE_REPOSITORY = 'fushangyun.tencentcloudcr.com/fushangyun/sample-service'
    }
    stages {
        stage('Checkout') {
            steps {
                deleteDir()
                checkout scm
                sh 'bash deploy/jenkins/release.sh validate'
            }
        }
        stage('集群前置检查') {
            when { expression { params.DEPLOY_TO_TKE } }
            steps {
                withCredentials([file(credentialsId: params.KUBECONFIG_CREDENTIALS_ID, variable: 'KUBECONFIG')]) {
                    sh 'bash deploy/jenkins/release.sh preflight'
                }
            }
        }
        stage('构建并推送镜像') {
            steps {
                withCredentials([usernamePassword(credentialsId: params.TCR_CREDENTIALS_ID, usernameVariable: 'TCR_USERNAME', passwordVariable: 'TCR_PASSWORD')]) {
                    sh 'bash deploy/jenkins/release.sh build'
                }
                archiveArtifacts(artifacts: 'release-artifacts/*', fingerprint: true)
            }
        }
        stage('发布到 TKE') {
            when { expression { params.DEPLOY_TO_TKE } }
            steps {
                withCredentials([file(credentialsId: params.KUBECONFIG_CREDENTIALS_ID, variable: 'KUBECONFIG')]) {
                    sh 'bash deploy/jenkins/release.sh deploy'
                }
            }
        }
    }
    post {
        always {
            archiveArtifacts(artifacts: 'release-artifacts/*', allowEmptyArchive: true, fingerprint: true)
        }
    }
}
```

质量检查加在「构建并推送镜像」之前。

同一目录新建 `deploy/jenkins/release.sh`。脚本里的 `sample-service`、Chart 路径、Dockerfile 路径、容器端口和 `/health` 与自己的仓库一致。BuildKit 镜像保持下面的 digest。

```bash
#!/usr/bin/env bash
# 从仓库根执行：bash deploy/jenkins/release.sh validate|preflight|build|deploy
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

chart=deploy/helm/sample-service
release=sample-service
artifacts=release-artifacts
image_repository="${IMAGE_REPOSITORY:-fushangyun.tencentcloudcr.com/fushangyun/sample-service}"
base_registry_host=fushangyun-vpc.tencentcloudcr.com
buildkit_image="$base_registry_host/base/buildkit@sha256:28a898719c18a33f4e8000685287fa36fd0dd9560c6440227d3a732d79bb41d8"

load_environment() {
    case "${DEPLOY_ENV:-}" in
        test) namespace=test ;;
        production) namespace=prod ;;
        *) echo 'DEPLOY_ENV 必须是 test 或 production' >&2; return 1 ;;
    esac
    environment_values="$chart/environments/$DEPLOY_ENV.json"
    jq -e --arg environment "$DEPLOY_ENV" --arg namespace "$namespace" \
        '.environment == $environment and .namespace == $namespace' "$environment_values" >/dev/null
    jq -es '[.[].ingress.host] | all(.[]; type == "string" and length > 0) and (unique | length == 2)' \
        "$chart/environments/test.json" "$chart/environments/production.json" >/dev/null
    app_url="https://$(jq -er '.ingress.host' "$environment_values")"
}

validate() {
    local tool missing=0
    for tool in git docker kubectl helm jq curl bash; do
        if ! command -v "$tool" >/dev/null; then
            printf '[缺失] %s\n' "$tool" >&2
            missing=1
        fi
    done
    [[ "$missing" == 0 ]]
    [[ "$(helm version --template '{{.Version}}')" == v4.* ]] || { echo '需要 Helm 4.x' >&2; return 1; }
    load_environment
    docker buildx version
    git diff --check
    bash -n deploy/jenkins/release.sh
}

preflight() {
    load_environment
    [[ -f "${KUBECONFIG:-}" ]] || { echo '缺少 Jenkins 上传的 kubeconfig' >&2; return 1; }
    local context="${KUBE_CONTEXT:-}" ingress clb config_secret tls_secret
    if [[ -z "$context" ]]; then
        context="$(kubectl --kubeconfig "$KUBECONFIG" config current-context)"
        [[ -n "$context" ]] || { echo 'kubeconfig 没有 current-context' >&2; return 1; }
    fi
    kube=(kubectl --kubeconfig "$KUBECONFIG" --context "$context" --namespace "$namespace" --request-timeout=30s)
    helm_target=(helm --kubeconfig "$KUBECONFIG" --kube-context "$context" --namespace "$namespace")
    clb="$(jq -er '.ingress.clbId' "$environment_values")"
    config_secret="$(jq -er '.configSecret' "$environment_values")"
    tls_secret="$(jq -er '.ingress.tlsSecret' "$environment_values")"
    ingress="$("${kube[@]}" get ingress "$release" --ignore-not-found -o json)"
    if [[ -n "$ingress" ]] && ! jq -e --arg clb "$clb" '
        .metadata.annotations["ingress.cloud.tencent.com/enable-group"] == "true"
        and .metadata.annotations["kubernetes.io/ingress.existLbId"] == $clb' <<< "$ingress" >/dev/null; then
        echo '现有 Ingress 未启用共享或绑定了其他 CLB' >&2
        return 1
    fi
    "${kube[@]}" get secret "$config_secret" -o go-template='{{if index .data "config.yaml"}}present{{end}}' | grep -qx present
    "${kube[@]}" get secret "$tls_secret" -o go-template='{{if index .data "qcloud_cert_id"}}present{{end}}' | grep -qx present
    if [[ -n "${IMAGE_PULL_SECRET:-}" ]]; then
        "${kube[@]}" get secret "$IMAGE_PULL_SECRET" >/dev/null
    fi
}

build() {
    load_environment
    mkdir -p "$artifacts"
    rm -f "$artifacts"/{chart.tgz,values-release.json,manifests.yaml,deployment-status.txt,helm-history.json,deployment.json,image-build.json}
    local commit tag digest
    docker_auth_dir="$(mktemp -d)"
    export DOCKER_CONFIG="$docker_auth_dir"
    builder_name="${release}-$(date +%s)-$$"
    builder_started=false
    cleanup_build() {
        local build_status=$?
        trap - EXIT
        if [[ "$builder_started" == true ]]; then
            docker buildx rm "$builder_name" >/dev/null 2>&1 || true
        fi
        rm -rf "$docker_auth_dir"
        exit "$build_status"
    }
    trap cleanup_build EXIT
    local registry
    for registry in "${REGISTRY_HOST:-fushangyun.tencentcloudcr.com}" "$base_registry_host"; do
        printf '%s' "$TCR_PASSWORD" | docker login "$registry" --username "$TCR_USERNAME" --password-stdin
    done
    builder_started=true
    docker buildx create --name "$builder_name" --driver docker-container \
        --driver-opt "image=$buildkit_image" --use >/dev/null
    commit="$(git rev-parse HEAD)"
    tag="${commit:0:12}-${DEPLOY_ENV}-${BUILD_NUMBER}"
    docker buildx build --builder "$builder_name" --platform "${IMAGE_PLATFORM:-linux/amd64}" \
        --file Dockerfile --tag "$image_repository:$tag" --push \
        --metadata-file "$artifacts/image-build.json" .
    digest="$(jq -er '."containerimage.digest" | strings | select(test("^sha256:[0-9a-f]{64}$"))' "$artifacts/image-build.json")"
    jq --arg image "$image_repository@$digest" \
        --arg release_id "$commit-$DEPLOY_ENV-$BUILD_NUMBER" \
        --arg pull "${IMAGE_PULL_SECRET:-}" \
        '.image = $image | .releaseId = $release_id | .imagePullSecret = $pull' \
        "$environment_values" > "$artifacts/values-release.json"
    helm lint "$chart" --strict --namespace "$namespace" -f "$artifacts/values-release.json"
    helm package "$chart" --destination "$docker_auth_dir"
    mv "$docker_auth_dir"/"${release}"-*.tgz "$artifacts/chart.tgz"
    helm template "$release" "$artifacts/chart.tgz" --namespace "$namespace" \
        -f "$artifacts/values-release.json" > "$artifacts/manifests.yaml"
}

deploy() {
    load_environment
    [[ -f "$artifacts/chart.tgz" && -f "$artifacts/values-release.json" ]] || { echo '缺少本次构建产物' >&2; return 1; }
    jq -e --slurpfile environment "$environment_values" --arg pull "${IMAGE_PULL_SECRET:-}" '
        [.environment, .namespace, .ingress, .configSecret, .imagePullSecret] ==
        [$environment[0].environment, $environment[0].namespace, $environment[0].ingress,
         $environment[0].configSecret, $pull]' "$artifacts/values-release.json" >/dev/null
    preflight
    helm template "$release" "$artifacts/chart.tgz" --namespace "$namespace" \
        -f "$artifacts/values-release.json" > "$artifacts/manifests.yaml"
    "${kube[@]}" apply --dry-run=server -f "$artifacts/manifests.yaml" >/dev/null
    "${helm_target[@]}" upgrade --install "$release" "$artifacts/chart.tgz" \
        -f "$artifacts/values-release.json" --reset-values --dry-run=server --hide-secret >/dev/null
    "${helm_target[@]}" upgrade --install "$release" "$artifacts/chart.tgz" \
        -f "$artifacts/values-release.json" --reset-values \
        --wait --rollback-on-failure --timeout 15m --history-max 20
    "${helm_target[@]}" history "$release" -o json > "$artifacts/helm-history.json"
    "${kube[@]}" rollout status "deployment/$release" --timeout=900s
    "${kube[@]}" get deployment "$release" -o json > "$artifacts/deployment.json"
    jq -e --slurpfile values "$artifacts/values-release.json" --arg name "$release" '
        [.spec.template.spec.containers[] | select(.name == $name) | .image] == [$values[0].image]
    ' "$artifacts/deployment.json" >/dev/null
    curl --fail --silent --show-error --retry 12 --retry-all-errors --retry-delay 10 \
        --connect-timeout 10 --max-time 20 --output /dev/null "$app_url/health"
    printf 'rollout、镜像核对与公网检查通过\n' > "$artifacts/deployment-status.txt"
}

case "${1:-}" in
    validate) validate ;;
    preflight) preflight ;;
    build) build ;;
    deploy) deploy ;;
    *) echo '用法: bash deploy/jenkins/release.sh validate|preflight|build|deploy' >&2; exit 2 ;;
esac
```

只在集群内被调用、不需要公网域名时：删除 `templates/ingress.yaml`，并删除脚本里 Ingress、证书 Secret 和 `curl` 这三段检查。

## 3. 准备 Helm Chart

Chart 放在应用仓库，随本次镜像一起打包。一个进程对应一个 Deployment。要再加一个进程，在 `templates/` 里加一份工作负载，并在发布脚本里增加它的镜像 digest。

`deploy/helm/sample-service/Chart.yaml`：

```yaml
apiVersion: v2
name: sample-service
description: sample-service on Tencent TKE
type: application
version: 0.1.0
```

`values.yaml`：

```yaml
environment: ""
namespace: ""
releaseId: ""
image: ""
imagePullSecret: ""
replicas: 1
containerPort: 8080
healthPath: /health
runAsUser: 10001
configSecret: ""
configRevision: "1"
resources:
  requests: {cpu: 100m, memory: 256Mi}
  limits: {cpu: "1", memory: 1Gi}
ingress:
  host: ""
  clbId: ""
  tlsSecret: ""
pdb:
  minAvailable: 0
```

`values.schema.json`：

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["environment", "namespace", "releaseId", "image", "imagePullSecret", "replicas", "containerPort", "healthPath", "runAsUser", "configSecret", "configRevision", "resources", "ingress", "pdb"],
  "properties": {
    "environment": {"enum": ["test", "production"]},
    "namespace": {"enum": ["test", "prod"]},
    "releaseId": {"type": "string", "minLength": 1},
    "image": {"type": "string", "pattern": "^[a-z0-9][a-z0-9.:/-]*@sha256:[0-9a-f]{64}$"},
    "imagePullSecret": {"type": "string", "pattern": "^$|^[a-z0-9]([-a-z0-9.]*[a-z0-9])?$"},
    "replicas": {"type": "integer", "minimum": 1},
    "containerPort": {"type": "integer", "minimum": 1, "maximum": 65535},
    "healthPath": {"type": "string", "minLength": 1},
    "runAsUser": {"type": "integer", "minimum": 1},
    "configSecret": {"type": "string", "minLength": 1},
    "configRevision": {"type": "string", "minLength": 1},
    "resources": {"type": "object"},
    "ingress": {
      "type": "object",
      "additionalProperties": false,
      "required": ["host", "clbId", "tlsSecret"],
      "properties": {
        "host": {"type": "string", "minLength": 1},
        "clbId": {"type": "string", "minLength": 1},
        "tlsSecret": {"type": "string", "minLength": 1}
      }
    },
    "pdb": {
      "type": "object",
      "additionalProperties": false,
      "required": ["minAvailable"],
      "properties": {"minAvailable": {"type": "integer", "minimum": 0}}
    }
  }
}
```

`environments/test.json`：

```json
{
  "environment": "test",
  "namespace": "test",
  "configSecret": "sample-service-config",
  "ingress": {
    "host": "sample-test.fushangyunfu.com",
    "clbId": "lb-gpk8k2ps",
    "tlsSecret": "fsytsl-wsk87cm7"
  }
}
```

`environments/production.json` 把 `environment` 改为 `production`，`namespace` 改为 `prod`，`host` 改为生产域名。两个域名不能相同。

`templates/app.yaml`：

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: sample-service
  namespace: {{ .Release.Namespace }}
  labels:
    app.kubernetes.io/managed-by: {{ .Release.Service }}
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/name: sample-service
automountServiceAccountToken: false
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sample-service
  namespace: {{ .Release.Namespace }}
  labels:
    app.kubernetes.io/managed-by: {{ .Release.Service }}
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/name: sample-service
spec:
  replicas: {{ .Values.replicas }}
  selector:
    matchLabels:
      app.kubernetes.io/name: sample-service
  template:
    metadata:
      annotations:
        app/release: {{ .Values.releaseId | quote }}
        app/config-revision: {{ .Values.configRevision | quote }}
      labels:
        app.kubernetes.io/name: sample-service
    spec:
{{- with .Values.imagePullSecret }}
      imagePullSecrets:
        - name: {{ . | quote }}
{{- end }}
      serviceAccountName: sample-service
      automountServiceAccountToken: false
      securityContext:
        runAsNonRoot: true
        runAsUser: {{ .Values.runAsUser }}
        runAsGroup: {{ .Values.runAsUser }}
        fsGroup: {{ .Values.runAsUser }}
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: sample-service
          image: {{ required "image 必须是 repository@sha256:digest" .Values.image | quote }}
          imagePullPolicy: IfNotPresent
          ports:
            - name: http
              containerPort: {{ .Values.containerPort }}
          env:
            - name: TZ
              value: Asia/Shanghai
          volumeMounts:
            - name: config
              mountPath: /app/config.yaml
              subPath: config.yaml
              readOnly: true
            - name: tmp
              mountPath: /tmp
          startupProbe:
            httpGet: {path: {{ .Values.healthPath | quote }}, port: http}
            periodSeconds: 10
            timeoutSeconds: 5
            failureThreshold: 60
          readinessProbe:
            httpGet: {path: {{ .Values.healthPath | quote }}, port: http}
            periodSeconds: 10
            timeoutSeconds: 3
            failureThreshold: 3
          livenessProbe:
            httpGet: {path: {{ .Values.healthPath | quote }}, port: http}
            periodSeconds: 20
            timeoutSeconds: 5
            failureThreshold: 3
          securityContext:
            allowPrivilegeEscalation: false
            capabilities:
              drop: ["ALL"]
          resources:
{{ toYaml .Values.resources | indent 12 }}
      volumes:
        - name: config
          secret:
            secretName: {{ .Values.configSecret | quote }}
            items:
              - key: config.yaml
                path: config.yaml
            defaultMode: 0444
        - name: tmp
          emptyDir: {}
---
apiVersion: v1
kind: Service
metadata:
  name: sample-service
  namespace: {{ .Release.Namespace }}
  labels:
    app.kubernetes.io/managed-by: {{ .Release.Service }}
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/name: sample-service
spec:
  type: NodePort
  selector:
    app.kubernetes.io/name: sample-service
  ports:
    - name: http
      port: {{ .Values.containerPort }}
      targetPort: http
```

`templates/ingress.yaml`：

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: sample-service
  namespace: {{ .Release.Namespace }}
  labels:
    app.kubernetes.io/managed-by: {{ .Release.Service }}
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/name: sample-service
  annotations:
    ingress.cloud.tencent.com/enable-group: "true"
    kubernetes.io/ingress.existLbId: {{ .Values.ingress.clbId | quote }}
    kubernetes.io/ingress.class: qcloud
    ingress.cloud.tencent.com/auto-rewrite: "true"
    ingress.cloud.tencent.com/tke-service-config: sample-service
spec:
  ingressClassName: qcloud
  tls:
    - hosts:
        - {{ .Values.ingress.host | quote }}
      secretName: {{ .Values.ingress.tlsSecret | quote }}
  rules:
    - host: {{ .Values.ingress.host | quote }}
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: sample-service
                port:
                  number: {{ .Values.containerPort }}
---
apiVersion: cloud.tencent.com/v1alpha1
kind: TkeServiceConfig
metadata:
  name: sample-service
  namespace: {{ .Release.Namespace }}
  labels:
    app.kubernetes.io/managed-by: {{ .Release.Service }}
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/name: sample-service
spec:
  loadBalancer:
    l7Listeners:
      - protocol: HTTPS
        port: 443
        domains:
          - domain: {{ .Values.ingress.host | quote }}
            rules:
              - url: /
                forwardType: HTTP
                healthCheck:
                  enable: true
                  intervalTime: 10
                  timeout: 5
                  healthNum: 3
                  unHealthNum: 3
                  httpCheckPath: {{ .Values.healthPath | quote }}
                  httpCheckMethod: GET
                  httpCheckDomain: {{ .Values.ingress.host | quote }}
                  httpCode: 2
```

`templates/pdb.yaml`：

```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: sample-service
  namespace: {{ .Release.Namespace }}
  labels:
    app.kubernetes.io/managed-by: {{ .Release.Service }}
    app.kubernetes.io/instance: {{ .Release.Name }}
    app.kubernetes.io/name: sample-service
spec:
  minAvailable: {{ .Values.pdb.minAvailable }}
  selector:
    matchLabels:
      app.kubernetes.io/name: sample-service
```

首次发布后不要改 Deployment 的 `selector`。程序必须监听 `0.0.0.0`，容器端口、Service 端口、探针端口使用同一个 `containerPort`。日志打到 stdout。密码和完整配置只放 Secret，不进 values、镜像和 Git。

配置没有独立文件时，删掉 ConfigMap/Secret 挂载，并同时改 schema、环境 JSON 和 `preflight` 里的 Secret 检查。

## 4. 准备 Dockerfile

仓库根目录放 `Dockerfile`。基础镜像使用 `fushangyun-vpc.tencentcloudcr.com/base` 里已经同步的镜像，并写死 digest。运行阶段示例：

```dockerfile
# syntax=fushangyun-vpc.tencentcloudcr.com/base/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
FROM fushangyun-vpc.tencentcloudcr.com/base/debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818
WORKDIR /app
COPY app /app/app
USER 10001
EXPOSE 8080
ENTRYPOINT ["/app/app"]
```

`USER` 与 Chart 的 `runAsUser` 保持一致。进程提供 `GET /health`，返回 2xx。

## 5. 发布前在腾讯云准备

1. 在 TCR 实例的 `fushangyun` 命名空间新建镜像仓库，名称与 `IMAGE_REPOSITORY` 最后一段相同。确认凭据 `tcr` 可以推送这个仓库，并可以拉取 `base`。
2. 在 TKE 的 `test`、`prod` 各建一份应用 Secret，名称与环境 JSON 的 `configSecret` 相同，数据键为 `config.yaml`。两份内容按环境分别填写。
3. 确认两个命名空间里都有证书 Secret `fsytsl-wsk87cm7`，并且证书覆盖本环境域名。Secret 不能跨命名空间引用。
4. 把测试域名和生产域名解析到 CLB `lb-gpk8k2ps` 的公网地址。域名和路径不能与已有应用冲突。
5. 从 Pod 里确认数据库、对象存储地址可访问。

改 Secret 内容后，把 `values.yaml` 的 `configRevision` 加 1 再发布。只改 Secret 不会更新已经挂进 Pod 的文件。

集群里如果已经有同名资源，而且不是这个 Helm release 创建的，先停下来。核对名称、selector、端口和当前镜像 digest 后，用同一套 Chart 做一次接管：

```bash
helm upgrade --install sample-service deploy/helm/sample-service \
  --namespace test \
  -f deploy/helm/sample-service/environments/test.json \
  --set-string image=<当前正在运行的 repository@sha256:digest> \
  --set-string releaseId=adopt-1 \
  --take-ownership --server-side=false --wait --timeout 15m --history-max 20
```

生产把命名空间和 values 换成 `prod`、`production.json`。这次不要加 `--rollback-on-failure`。接管完成后再用 Jenkins 发布。

## 6. 创建 Jenkins 任务并发布

1. 新建 Pipeline 任务，例如 `sample-service-k8s`。一个环境不要再并行另一个任务。
2. 选择 `Pipeline script from SCM`，填应用仓库、Git 凭据和发布分支。Script Path 填 `Jenkinsfile.k8s`。
3. 把第 2 到第 4 步的文件推上该分支。
4. `Build with Parameters`。第一次选 `DEPLOY_ENV=test`，`DEPLOY_TO_TKE=true`。
5. 测试通过后，再显式选择 `production`。

流水线用本次构建的 `repository@sha256:digest` 发布，不使用 `latest`。升级失败时 Helm 会回退到上一成功版本。第一次安装没有上一版本，失败可能清掉刚创建的资源。

## 7. 验收和回退

发布成功时，构建归档里要有 `deployment-status.txt`。同时满足：

- Helm release `sample-service` 状态为 deployed。
- Deployment 镜像等于本次 `values-release.json` 里的 digest。
- `https://<本环境域名>/health` 返回成功。

回退时，选一次带 `deployment-status.txt` 的成功构建，打开它的 `helm-history.json` 确定 revision，然后：

```bash
helm --kubeconfig "$KUBECONFIG" --kube-context "$KUBE_CONTEXT" -n test rollback sample-service <revision> --wait --timeout 15m
kubectl --kubeconfig "$KUBECONFIG" --context "$KUBE_CONTEXT" -n test rollout status deployment/sample-service --timeout=900s
```

生产把命名空间换成 `prod`。回退之后再访问一次 `/health`。回退不会恢复 Secret，也不会撤销数据库变更。

之后只改业务代码时，重新跑这个 Jenkins 任务。改端口、探针、环境变量或增加进程时，和代码放在同一次提交里改 Chart，再跑同一个任务。
