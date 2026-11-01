//! 集成测试共享辅助函数。

use std::time::{SystemTime, UNIX_EPOCH};

/// 从环境变量读取 MongoDB 连接串，未设置时打印原因并返回 `None`。
///
/// # 返回值
/// 设置 `ERP_TEST_MONGO_URI` 时返回连接串，否则返回 `None`。
pub fn mongo_uri() -> Option<String> {
    match std::env::var("ERP_TEST_MONGO_URI") {
        Ok(uri) if !uri.trim().is_empty() => Some(uri),
        _ => {
            eprintln!("跳过集成测试：未设置环境变量 ERP_TEST_MONGO_URI");
            None
        }
    }
}

/// 生成本次测试专用的独立数据库名（进程号 + 纳秒时间戳）。
///
/// 避免与其他并行测试或并行 agent 的测试数据互扰。
///
/// # 参数
/// * `prefix` - 测试用途标识
///
/// # 返回值
/// 形如 `{prefix}_{pid}_{nanos}` 的数据库名。
pub fn unique_database_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    format!("{prefix}_{}_{}", std::process::id(), nanos)
}

/// 从业务编号字符串解析序号段。
///
/// # 参数
/// * `number` - 形如 `SO20260701-000123` 的编号
///
/// # 返回值
/// 返回连字符后的序号段数值。
pub fn seq_of(number: &str) -> i64 {
    number
        .rsplit_once('-')
        .expect("编号必须包含连字符分隔符")
        .1
        .parse()
        .expect("序号段必须是数字")
}
