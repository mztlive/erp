#!/usr/bin/env bash
# 独立上传到腾讯云 CVM 后执行：bash sync-erp-images.sh
# 前置条件：安装 skopeo，创建 TCR base 命名空间及下列五个仓库。
# 同一 Linux 用户先登录：skopeo login fushangyun-vpc.tencentcloudcr.com
# 源端采用腾讯云 Docker Hub 内网加速器；目标端采用企业 TCR 内网域名。
# 保留源镜像的全部架构及 digest；不自动回退外部源或更换镜像版本。
# BuildKit 使用 sync-buildkit-image.sh 单独同步；业务镜像由 Jenkins 构建。
set -euo pipefail

SOURCE_REGISTRY='mirror.ccs.tencentyun.com'
TCR_BASE='fushangyun-vpc.tencentcloudcr.com/base'
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DIGEST_DIR="$SCRIPT_DIR/image-sync-digests"

command -v skopeo >/dev/null || {
  echo '未安装 skopeo，请先安装后重试。' >&2
  exit 1
}
mkdir -p "$DIGEST_DIR"

# 按原始摘要或标签复制镜像，并保存本次成功推送的摘要。
sync_image() {
  local source_image="$SOURCE_REGISTRY/$1"
  local target_image="$TCR_BASE/$2"
  local name="$3"
  local status

  printf '\n正在同步 %s → %s\n' "$source_image" "$target_image"
  rm -f -- "$DIGEST_DIR/$name.digest"

  if skopeo copy \
    --all \
    --preserve-digests \
    --retry-times 3 \
    --digestfile "$DIGEST_DIR/$name.digest" \
    "docker://$source_image" \
    "docker://$target_image"; then
    printf '同步成功：%s\n' "$target_image"
  else
    status=$?
    rm -f -- "$DIGEST_DIR/$name.digest"
    printf '\n同步失败：%s，退出码：%s。\n' "$name" "$status" >&2
    echo '请检查上方原始错误：manifest unknown 表示加速器未提供所需镜像；网络错误请检查内网解析和连通性；权限错误请检查 TCR 登录与仓库权限。' >&2
    echo '解决后重新运行脚本。脚本不会自动改用其他版本。' >&2
    exit "$status"
  fi
}

sync_image \
  'library/rust@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777' \
  'rust:1.97-slim-bookworm' \
  'rust'

sync_image \
  'library/debian@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818' \
  'debian:bookworm-slim' \
  'debian'

sync_image 'library/node@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c' 'node:22-bookworm-slim' 'node'
sync_image 'library/mongo@sha256:9854f7139445d766a9523571d6f047530c45547460ffcf8259eb2bf4264632ca' 'mongo:7' 'mongo'
sync_image 'docker/dockerfile@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e' 'dockerfile:1.7' 'dockerfile'

printf '\n五个基础镜像全部同步完成。digest 保存于：%s\n' "$DIGEST_DIR"
printf 'BuildKit 请使用 sync-buildkit-image.sh 单独同步；构建验收通过后再发布。\n'
