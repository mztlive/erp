//! 客户资料服务端业务编号生成。

use id_generator::next_id;

/// 用调用方前缀和服务端 ID 生成业务编号，不接受客户端编号。
///
/// # 参数
/// * `prefix` - 编号前缀，原样拼在 ID 之前。
///
/// # 返回
/// 返回 `{prefix}-{id}` 形式的业务编号。
///
/// # 错误
/// 不返回错误。
pub(super) fn business_no(prefix: &str) -> String {
    format!("{prefix}-{}", next_id())
}
