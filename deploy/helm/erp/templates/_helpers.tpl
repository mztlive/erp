{{/* 固定资源名称与 selector，接管现有资源时不得改变。每个命名空间仅允许一个 erp release。 */}}
{{- define "erp.validate" -}}
{{- if ne .Release.Name "erp" }}{{ fail "release 名称必须是 erp" }}{{ end -}}
{{- if ne .Release.Namespace .Values.namespace }}{{ fail "release namespace 与环境配置不一致" }}{{ end -}}
{{- if and (eq .Values.environment "production") (ne .Values.namespace "prod") }}{{ fail "production 必须部署到 prod" }}{{ end -}}
{{- if and (eq .Values.environment "test") (ne .Values.namespace "test") }}{{ fail "test 必须部署到 test" }}{{ end -}}
{{- if eq .Values.ingress.apiHost .Values.ingress.webHost }}{{ fail "API 与管理端域名必须不同" }}{{ end -}}
{{- if and (eq .Values.environment "test") (ne .Values.api.nacos.namespace "ccf7ec38-1d60-407e-bf2c-7c4654c481d0") }}{{ fail "test 必须使用测试 Nacos Namespace UUID" }}{{ end -}}
{{- if and (eq .Values.environment "production") (ne .Values.api.nacos.namespace "8888c735-1d29-4765-ab7f-70100a888479") }}{{ fail "production 必须使用生产 Nacos Namespace UUID" }}{{ end -}}
{{- end -}}
