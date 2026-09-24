{{/* 固定资源名称与 selector，接管现有资源时不得改变。每个命名空间仅允许一个 erp release。 */}}
{{- define "erp.validate" -}}
{{- if ne .Release.Name "erp" }}{{ fail "release 名称必须是 erp" }}{{ end -}}
{{- if ne .Release.Namespace .Values.namespace }}{{ fail "release namespace 与环境配置不一致" }}{{ end -}}
{{- if and (eq .Values.environment "production") (ne .Values.namespace "prod") }}{{ fail "production 必须部署到 prod" }}{{ end -}}
{{- if and (eq .Values.environment "test") (ne .Values.namespace "test") }}{{ fail "test 必须部署到 test" }}{{ end -}}
{{- if eq .Values.ingress.apiHost .Values.ingress.webHost }}{{ fail "API 与管理端域名必须不同" }}{{ end -}}
{{- end -}}
