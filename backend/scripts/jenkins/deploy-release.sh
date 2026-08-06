#!/usr/bin/env bash
# Transfer and deploy one digest-pinned rs-project-template release from Jenkins.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

: "${WORKSPACE:?Jenkins must provide WORKSPACE}"
: "${BUILD_NUMBER:?Jenkins must provide BUILD_NUMBER}"
: "${DEPLOY_TARGET:?Jenkins must provide DEPLOY_TARGET}"
: "${DEPLOY_DIR:?Jenkins must provide DEPLOY_DIR}"

RS_PROJECT_TEMPLATE_API_HOST_PORT="${RS_PROJECT_TEMPLATE_API_HOST_PORT:-10001}"

validate_port() {
  local name="$1" value="$2" port
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    echo "$name must be an integer from 1 to 65535: $value" >&2
    exit 2
  fi
  port=$((10#$value))
  if ((port < 1 || port > 65535)); then
    echo "$name must be an integer from 1 to 65535: $value" >&2
    exit 2
  fi
}

validate_port RS_PROJECT_TEMPLATE_API_HOST_PORT "$RS_PROJECT_TEMPLATE_API_HOST_PORT"
RS_PROJECT_TEMPLATE_API_HOST_PORT=$((10#$RS_PROJECT_TEMPLATE_API_HOST_PORT))

if [[ ! "$DEPLOY_TARGET" =~ ^[A-Za-z0-9._-]+@[A-Za-z0-9.-]+$ ]]; then
  echo "DEPLOY_TARGET must use the user@host form: $DEPLOY_TARGET" >&2
  exit 2
fi
if [[ ! "$DEPLOY_DIR" =~ ^/[A-Za-z0-9._/-]+$ ||
      "$DEPLOY_DIR" == "/" ||
      "$DEPLOY_DIR" == *"/../"* ||
      "$DEPLOY_DIR" == *"/.." ||
      "$DEPLOY_DIR" == *"/./"* ]]; then
  echo "DEPLOY_DIR must be a safe absolute path: $DEPLOY_DIR" >&2
  exit 2
fi

commit="$(git -C "$ROOT_DIR" rev-parse --verify HEAD)"
tag="${commit}-${BUILD_NUMBER}"
release_dir="${RS_PROJECT_TEMPLATE_RELEASE_OUTPUT_DIR:-$WORKSPACE/release-artifacts}"
release_manifest="$release_dir/$tag.env"
deploy_manifest="$release_dir/$tag.deploy.env"

test -f "$release_manifest"
umask 077
cp "$release_manifest" "$deploy_manifest"
{
  printf '\n%s\n' '# Jenkins deployment parameters; safe to retain for rollback.'
  printf 'RS_PROJECT_TEMPLATE_API_HOST_PORT=%s\n' "$RS_PROJECT_TEMPLATE_API_HOST_PORT"
} >>"$deploy_manifest"

ssh "$DEPLOY_TARGET" bash -s -- "$DEPLOY_DIR" <<'REMOTE_PREPARE'
set -euo pipefail
deploy_dir="$1"
mkdir -p "$deploy_dir/scripts" "$deploy_dir/incoming"
test -f "$deploy_dir/config.toml" || {
  echo "production config is missing: $deploy_dir/config.toml" >&2
  exit 2
}
REMOTE_PREPARE

scp "$ROOT_DIR/docker-compose.production.yml" \
  "$DEPLOY_TARGET:$DEPLOY_DIR/docker-compose.production.yml"
scp "$ROOT_DIR/scripts/release-deploy.sh" \
  "$DEPLOY_TARGET:$DEPLOY_DIR/scripts/release-deploy.sh"
scp "$deploy_manifest" \
  "$DEPLOY_TARGET:$DEPLOY_DIR/incoming/$tag.env"

ssh "$DEPLOY_TARGET" bash -s -- "$DEPLOY_DIR" "$tag" <<'REMOTE_DEPLOY'
set -euo pipefail
deploy_dir="$1"
tag="$2"
cd "$deploy_dir"
chmod 750 scripts/release-deploy.sh
scripts/release-deploy.sh deploy "incoming/$tag.env"
scripts/release-deploy.sh current
docker compose \
  --env-file .release-state/current.env \
  -f docker-compose.production.yml \
  ps
REMOTE_DEPLOY
