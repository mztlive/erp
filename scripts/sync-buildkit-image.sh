#!/usr/bin/env bash
# 在腾讯云 CVM 执行；预先创建 TCR base/buildkit 仓库。
# 登录：skopeo login fushangyun-vpc.tencentcloudcr.com
# 同步：bash sync-buildkit-image.sh
# 默认复制已同步并在 release.sh 固定的 BuildKit digest。
# 升级时显式指定源版本，成功后同步更新 release.sh：BUILDKIT_SOURCE_REF='sha256:...' bash sync-buildkit-image.sh
set -euo pipefail

SOURCE_REPOSITORY='mirror.ccs.tencentyun.com/moby/buildkit'
TARGET_REPOSITORY='fushangyun-vpc.tencentcloudcr.com/base/buildkit'
SOURCE_REF="${BUILDKIT_SOURCE_REF:-sha256:28a898719c18a33f4e8000685287fa36fd0dd9560c6440227d3a732d79bb41d8}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DIGEST_DIR="$SCRIPT_DIR/image-sync-digests"

command -v skopeo >/dev/null || {
  echo '未安装 skopeo，请先安装。' >&2
  exit 1
}
if [[ "$SOURCE_REF" == sha256:* ]]; then
  SOURCE_IMAGE="$SOURCE_REPOSITORY@$SOURCE_REF"
else
  SOURCE_IMAGE="$SOURCE_REPOSITORY:$SOURCE_REF"
fi

mkdir -p "$DIGEST_DIR"
rm -f -- "$DIGEST_DIR/buildkit.digest" "$DIGEST_DIR/buildkit-image.txt"
printf '正在同步 %s → %s:buildx-stable-1\n' "$SOURCE_IMAGE" "$TARGET_REPOSITORY"

if skopeo copy \
  --all \
  --preserve-digests \
  --retry-times 3 \
  --digestfile "$DIGEST_DIR/buildkit.digest" \
  "docker://$SOURCE_IMAGE" \
  "docker://$TARGET_REPOSITORY:buildx-stable-1"; then
  DIGEST="$(cat "$DIGEST_DIR/buildkit.digest")"
  if [[ ! "$DIGEST" =~ ^sha256:[a-f0-9]{64}$ ]]; then
    echo '同步返回的 digest 格式异常，请检查结果。' >&2
    exit 1
  fi
  printf '%s@%s\n' "$TARGET_REPOSITORY" "$DIGEST" > "$DIGEST_DIR/buildkit-image.txt"
  printf '\n同步完成。Jenkins 应使用以下固定镜像地址：\n'
  cat "$DIGEST_DIR/buildkit-image.txt"
  printf '\n请保留整个 %s 目录，用于后续配置镜像引用。\n' "$DIGEST_DIR"
else
  status=$?
  rm -f -- "$DIGEST_DIR/buildkit.digest"
  echo '同步失败，请检查上方源站、网络或 TCR 权限错误，解决后重试。' >&2
  exit "$status"
fi
