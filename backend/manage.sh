#!/usr/bin/env bash
# Local Docker Compose management for rs-project-template.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

export RS_PROJECT_TEMPLATE_LOCAL_UID="${RS_PROJECT_TEMPLATE_LOCAL_UID:-$(id -u)}"
export RS_PROJECT_TEMPLATE_LOCAL_GID="${RS_PROJECT_TEMPLATE_LOCAL_GID:-$(id -g)}"

usage() {
  cat <<'EOF'
usage: ./manage.sh <command> [service]

commands:
  start              Build and start web-api
  stop               Stop the local stack
  restart            Restart the local stack
  status             Show service status
  logs [service]     Follow all logs or one service log
  build              Build local images
  health             Check Web API health
  shell <service>    Open a shell in a running service
  help               Show this help

services: web-api
EOF
}

require_docker() {
  command -v docker >/dev/null 2>&1 || {
    echo "docker is not installed" >&2
    exit 1
  }
  docker compose version >/dev/null 2>&1 || {
    echo "Docker Compose v2 is unavailable" >&2
    exit 1
  }
  docker info >/dev/null 2>&1 || {
    echo "Docker daemon is not running" >&2
    exit 1
  }
}

prepare_paths() {
  [[ -f "${RS_PROJECT_TEMPLATE_CONFIG_FILE:-$ROOT_DIR/config.toml}" ]] || {
    echo "config.toml is missing; copy config.toml.example and fill real values" >&2
    exit 2
  }
}

health() {
  docker compose exec -T web-api \
    curl --fail --silent --show-error --max-time 3 \
    http://127.0.0.1:10001/health >/dev/null
  echo "web-api is healthy"
}

main() {
  require_docker
  case "${1:-help}" in
    start)
      prepare_paths
      docker compose up -d --build
      docker compose ps
      ;;
    stop)
      docker compose down
      ;;
    restart)
      docker compose restart
      docker compose ps
      ;;
    status)
      docker compose ps
      ;;
    logs)
      if [[ -n "${2:-}" ]]; then
        docker compose logs -f --tail=100 "$2"
      else
        docker compose logs -f --tail=100
      fi
      ;;
    build)
      prepare_paths
      docker compose build
      ;;
    health)
      health
      ;;
    shell)
      [[ -n "${2:-}" ]] || {
        echo "service is required" >&2
        usage
        exit 2
      }
      docker compose exec "$2" /bin/sh
      ;;
    help)
      usage
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
