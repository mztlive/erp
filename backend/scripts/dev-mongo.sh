#!/usr/bin/env bash
# P0-3 本地 MongoDB 单节点副本集启动脚本（集成测试专用）。
#
# 事务需要副本集（standalone 不支持），因此以 --replSet rs0 启动 mongo:7
# 容器并自动执行 rs.initiate()。幂等：容器已存在时直接复用。
#
# 用法：
#   ./backend/scripts/dev-mongo.sh
#   export ERP_TEST_MONGO_URI='mongodb://127.0.0.1:27017/?replicaSet=rs0'
#   cargo test --workspace -- --include-ignored
#
# 停止：docker stop erp-dev-mongo（--rm 容器停止后自动移除）
set -euo pipefail

CONTAINER_NAME="${ERP_DEV_MONGO_CONTAINER:-erp-dev-mongo}"
IMAGE="${ERP_DEV_MONGO_IMAGE:-mongo:7}"
HOST_PORT="${ERP_DEV_MONGO_PORT:-27017}"
VOLUME_NAME="erp-dev-mongo-data"
MONGO_URI="mongodb://127.0.0.1:${HOST_PORT}/?replicaSet=rs0"
READY_TIMEOUT_SECONDS=60
INIT_ATTEMPTS=10

ensure_docker() {
    if ! command -v docker >/dev/null 2>&1; then
        echo "错误: 未找到 docker 命令，请先安装 Docker。" >&2
        exit 1
    fi
}

start_container() {
    if docker ps --format '{{.Names}}' | grep -qx "${CONTAINER_NAME}"; then
        echo "已存在运行中的容器 ${CONTAINER_NAME}，直接复用。"
        return
    fi
    if docker ps -a --format '{{.Names}}' | grep -qx "${CONTAINER_NAME}"; then
        echo "容器 ${CONTAINER_NAME} 存在但未运行，重新启动。"
        docker start "${CONTAINER_NAME}" >/dev/null
        return
    fi
    echo "启动容器 ${CONTAINER_NAME}（mongo:7，--replSet rs0，端口 ${HOST_PORT}）。"
    docker run \
        --name "${CONTAINER_NAME}" \
        --rm \
        -v "${VOLUME_NAME}:/data/db" \
        -p "${HOST_PORT}:27017" \
        -d "${IMAGE}" \
        --replSet rs0 >/dev/null
}

wait_ready() {
    echo "等待 MongoDB 就绪（最长 ${READY_TIMEOUT_SECONDS} 秒）…"
    for _ in $(seq 1 "${READY_TIMEOUT_SECONDS}"); do
        if docker exec "${CONTAINER_NAME}" mongosh --quiet --eval 'db.runCommand({ ping: 1 }).ok' | grep -q 1; then
            return
        fi
        sleep 1
    done
    echo "错误: MongoDB 在 ${READY_TIMEOUT_SECONDS} 秒内未就绪。" >&2
    exit 1
}

init_replica_set() {
    if docker exec "${CONTAINER_NAME}" mongosh --quiet --eval 'db.hello().setName' | grep -q rs0; then
        echo "副本集 rs0 已初始化，跳过 rs.initiate()。"
        return
    fi
    echo "初始化单节点副本集 rs0（失败会自动重试 ${INIT_ATTEMPTS} 次）…"
    for attempt in $(seq 1 "${INIT_ATTEMPTS}"); do
        if docker exec "${CONTAINER_NAME}" mongosh --quiet --eval \
            "rs.initiate({ _id: 'rs0', members: [{ _id: 0, host: '127.0.0.1:${HOST_PORT}' }] })" \
            >/dev/null 2>&1; then
            return
        fi
        echo "  rs.initiate() 第 ${attempt} 次尝试未成功，1 秒后重试…"
        sleep 1
    done
    echo "错误: rs.initiate() 多次尝试后仍失败。" >&2
    exit 1
}

wait_primary() {
    echo "等待副本集选出 PRIMARY…"
    for _ in $(seq 1 "${READY_TIMEOUT_SECONDS}"); do
        if docker exec "${CONTAINER_NAME}" mongosh --quiet --eval 'db.hello().isWritablePrimary' | grep -q true; then
            return
        fi
        sleep 1
    done
    echo "错误: 副本集未能在 ${READY_TIMEOUT_SECONDS} 秒内选出 PRIMARY。" >&2
    exit 1
}

print_usage() {
    echo ""
    echo "MongoDB 单节点副本集已就绪。"
    echo ""
    echo "连接串: ${MONGO_URI}"
    echo ""
    echo "使用说明："
    echo "  export ERP_TEST_MONGO_URI='${MONGO_URI}'"
    echo "  cargo test --workspace -- --include-ignored   # 运行全部集成测试"
    echo "  docker stop ${CONTAINER_NAME}                 # 停止并自动移除容器"
}

ensure_docker
start_container
wait_ready
init_replica_set
wait_primary
print_usage
