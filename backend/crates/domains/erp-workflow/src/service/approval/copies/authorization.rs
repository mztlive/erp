//! 审批抄送只提供真实收件事实和静态映射。
use std::sync::Arc;

use erp_identity::{AccessFactsPort, AccessMapping, Result};
use mongodb::Database;

use crate::repository::approval_copy::authorization::{CopyFacts, CopyMapping};
/// 构造抄送事实读取器。
/// # 参数
/// db 为工作流数据库。
/// # 返回
/// 真实送达记录和收件人的事实读取器。
/// # 错误
/// 构造不执行 I/O。
pub fn authorization_facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(CopyFacts(db))
}
/// 构造抄送静态查询映射。
/// # 参数
/// 无。
/// # 返回
/// 固定抄送集合映射。
/// # 错误
/// 模型非法时拒绝。
pub fn authorization_mapping() -> Result<Arc<dyn AccessMapping>> {
    CopyMapping::shared()
}
