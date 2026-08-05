#!/usr/bin/env bash
# Build and push one immutable rs-project-template image set from Jenkins.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

: "${WORKSPACE:?Jenkins must provide WORKSPACE}"
: "${BUILD_NUMBER:?Jenkins must provide BUILD_NUMBER}"
: "${REGISTRY_HOST:?Jenkins must provide REGISTRY_HOST}"
: "${RS_PROJECT_TEMPLATE_ADMIN_API_BASE_URL:?Jenkins must provide RS_PROJECT_TEMPLATE_ADMIN_API_BASE_URL}"

if [[ ! "$REGISTRY_HOST" =~ ^[A-Za-z0-9.-]+(:[0-9]{1,5})?$ ]]; then
  echo "REGISTRY_HOST must be a host name or host:port: $REGISTRY_HOST" >&2
  exit 2
fi
if [[ "$REGISTRY_HOST" == *:* ]]; then
  registry_port="${REGISTRY_HOST##*:}"
  registry_port_number=$((10#$registry_port))
  if ((registry_port_number < 1 || registry_port_number > 65535)); then
    echo "REGISTRY_HOST port is out of range: $registry_port" >&2
    exit 2
  fi
fi

export RS_PROJECT_TEMPLATE_BACKEND_IMAGE_REPOSITORY="$REGISTRY_HOST/rs-project-template-backend"
export RS_PROJECT_TEMPLATE_ADMIN_IMAGE_REPOSITORY="$REGISTRY_HOST/rs-project-template-admin"
commit="$(git -C "$ROOT_DIR" rev-parse --verify HEAD)"
export RS_PROJECT_TEMPLATE_RELEASE_TAG="${commit}-${BUILD_NUMBER}"
export RS_PROJECT_TEMPLATE_RELEASE_OUTPUT_DIR="${RS_PROJECT_TEMPLATE_RELEASE_OUTPUT_DIR:-$WORKSPACE/release-artifacts}"

printf 'release registry: %s\n' "$REGISTRY_HOST"
"$ROOT_DIR/scripts/release-image.sh" build
"$ROOT_DIR/scripts/release-image.sh" push

manifest="$RS_PROJECT_TEMPLATE_RELEASE_OUTPUT_DIR/$RS_PROJECT_TEMPLATE_RELEASE_TAG.env"
test -f "$manifest"
printf 'release manifest ready: %s\n' "$manifest"
