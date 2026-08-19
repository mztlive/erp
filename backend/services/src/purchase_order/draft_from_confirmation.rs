//! 旧采购确认拆单入口已删除。选源由采购单创建路径承担。

use database::Executor;
use entities::ids::SalesOrderId;
use mongodb::Database;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 确认拆单后落库（或幂等复用）的采购草稿身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedPurchaseDraft {
    /// 采购单 ID。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 乐观锁版本。
    pub lock_version: u64,
    /// 是否幂等回放。
    pub replayed: bool,
}

/// 旧确认拆单入口。恒失败关闭。
///
/// # 错误
/// 恒返回业务错误，不得回退旧确认批次。
pub(crate) async fn create_drafts_from_confirmation_lines(
    _db: &Database,
    _sales_order_id: &SalesOrderId,
    _actor: &AuditActor,
    _executor: &mut dyn Executor,
) -> Result<Vec<CreatedPurchaseDraft>> {
    Err(Error::BusinessLogicError("旧采购确认拆单入口已删除".to_string()))
}
