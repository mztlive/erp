//! 客户资料服务端业务编号生成。

use id_generator::next_id;

/// 生成服务端业务编号。
pub(super) fn business_no(prefix: &str) -> String {
    format!("{prefix}-{}", next_id())
}
