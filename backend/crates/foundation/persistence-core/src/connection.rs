use mongodb::bson::{Bson, Document, doc};
use mongodb::{Client, Database};

use crate::{Error, Result};

const MIN_REPLICA_SET_TRANSACTION_WIRE_VERSION: i64 = 7;
const MIN_SHARDED_TRANSACTION_WIRE_VERSION: i64 = 8;

/// 应用装配时从 URI 创建共享客户端并选定数据库句柄；成功只表示句柄可创建，不证明连接健康或支持事务，
/// 启动时另用 [`ensure_transaction_support`] 验证。
pub async fn connect(uri: &str, db_name: &str) -> Result<(Client, Database)> {
    let client = mongodb::Client::with_uri_str(uri).await?;
    let database = client.database(db_name);
    Ok((client, database))
}

/// 启动时用 `hello` 检查会话和事务所需的部署能力。仅支持相应版本的副本集或分片集群；
/// standalone、会话不可用或命令失败时拒绝继续。
pub async fn ensure_transaction_support(database: &Database) -> Result<()> {
    let hello = database.run_command(doc! { "hello": 1 }).await?;
    if topology_supports_transactions(&hello) {
        return Ok(());
    }
    Err(Error::UnsupportedDeployment("a replica set or transaction-capable sharded cluster is required"))
}

// 事务部署检查以非负整数的 `logicalSessionTimeoutMinutes` 判断逻辑会话可用，缺少该字段或格式异常时不放行。
fn sessions_enabled(hello: &Document) -> bool {
    hello.get("logicalSessionTimeoutMinutes").and_then(bson_integer).is_some_and(|minutes| minutes >= 0)
}

// 按 `hello` 的 `msg: isdbgrid` 识别 `mongos`，以便应用分片事务的版本门槛。
fn is_sharded_cluster(hello: &Document) -> bool {
    hello.get_str("msg").is_ok_and(|value| value == "isdbgrid")
}

// 以 `hello` 中非空的 `setName` 识别副本集，区别于无法运行多文档事务的 standalone。
fn is_replica_set(hello: &Document) -> bool {
    hello.get_str("setName").is_ok_and(|value| !value.trim().is_empty())
}

// 应用的多文档事务要求会话已启用。副本集需 wire version 至少为 7（MongoDB 4.0），分片集群至少为 8（4.2）；
// 响应缺少关键信息时拒绝。
fn topology_supports_transactions(hello: &Document) -> bool {
    if !sessions_enabled(hello) {
        return false;
    }
    let Some(max_wire_version) = hello.get("maxWireVersion").and_then(bson_integer) else {
        return false;
    };

    (is_replica_set(hello) && max_wire_version >= MIN_REPLICA_SET_TRANSACTION_WIRE_VERSION)
        || (is_sharded_cluster(hello) && max_wire_version >= MIN_SHARDED_TRANSACTION_WIRE_VERSION)
}

// 读取 `hello` 的数值字段时兼容 BSON Int32 和 Int64，不接受浮点或字符串冒充整数。
fn bson_integer(value: &Bson) -> Option<i64> {
    match value {
        Bson::Int32(value) => Some(i64::from(*value)),
        Bson::Int64(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::topology_supports_transactions;

    #[test]
    fn replica_set_with_sessions_supports_transactions() {
        let hello = doc! {
            "setName": "rs0",
            "logicalSessionTimeoutMinutes": 30,
            "maxWireVersion": 7,
        };

        assert!(topology_supports_transactions(&hello));
    }

    #[test]
    fn transaction_capable_mongos_is_supported() {
        let hello = doc! {
            "msg": "isdbgrid",
            "logicalSessionTimeoutMinutes": 30,
            "maxWireVersion": 8_i64,
        };

        assert!(topology_supports_transactions(&hello));
    }

    #[test]
    fn standalone_or_old_deployment_is_rejected() {
        let standalone = doc! {
            "logicalSessionTimeoutMinutes": 30,
            "maxWireVersion": 17_i64,
        };
        let old_replica_set = doc! {
            "setName": "rs0",
            "logicalSessionTimeoutMinutes": 30,
            "maxWireVersion": 6_i64,
        };
        let sessions_disabled = doc! {
            "setName": "rs0",
            "logicalSessionTimeoutMinutes": null,
            "maxWireVersion": 17,
        };

        assert!(!topology_supports_transactions(&standalone));
        assert!(!topology_supports_transactions(&old_replica_set));
        assert!(!topology_supports_transactions(&sessions_disabled));
    }
}
