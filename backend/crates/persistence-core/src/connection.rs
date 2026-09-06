use crate::{Error, Result};
use mongodb::{
    bson::{doc, Bson, Document},
    Client, Database,
};

const MIN_REPLICA_SET_TRANSACTION_WIRE_VERSION: i64 = 7;
const MIN_SHARDED_TRANSACTION_WIRE_VERSION: i64 = 8;

/// 连接 MongoDB 并返回共享客户端及目标数据库句柄。
///
/// # 参数
/// * `uri` - MongoDB 连接地址
/// * `db_name` - 目标数据库名
///
/// # 错误
/// 当客户端创建或连接参数解析失败时返回错误。
pub async fn connect(uri: &str, db_name: &str) -> Result<(Client, Database)> {
    let client = mongodb::Client::with_uri_str(uri).await?;
    let database = client.database(db_name);
    Ok((client, database))
}

/// 验证当前 MongoDB 部署支持本项目依赖的多文档事务。
///
/// # 参数
/// * `database` - 已配置的目标数据库
///
/// # 返回值
/// 副本集或支持事务的分片集群返回成功。
///
/// # 错误
/// 连接、命令执行失败，或目标为 standalone/不支持会话与事务时返回错误。
pub async fn ensure_transaction_support(database: &Database) -> Result<()> {
    let hello = database.run_command(doc! { "hello": 1 }).await?;
    if topology_supports_transactions(&hello) {
        return Ok(());
    }

    Err(Error::UnsupportedDeployment(
        "a replica set or transaction-capable sharded cluster is required",
    ))
}

/// 判断 `hello` 响应描述的拓扑是否具备事务所需的会话和 wire version。
fn topology_supports_transactions(hello: &Document) -> bool {
    if !hello
        .get("logicalSessionTimeoutMinutes")
        .and_then(bson_integer)
        .is_some_and(|minutes| minutes >= 0)
    {
        return false;
    }
    let Some(max_wire_version) = hello.get("maxWireVersion").and_then(bson_integer) else {
        return false;
    };
    let is_sharded = hello.get_str("msg").is_ok_and(|value| value == "isdbgrid");
    let is_replica_set = hello
        .get_str("setName")
        .is_ok_and(|value| !value.trim().is_empty());

    (is_replica_set && max_wire_version >= MIN_REPLICA_SET_TRANSACTION_WIRE_VERSION)
        || (is_sharded && max_wire_version >= MIN_SHARDED_TRANSACTION_WIRE_VERSION)
}

/// 将 BSON 的两种有符号整数表示统一为 `i64`。
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
