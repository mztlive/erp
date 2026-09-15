//! 采购变更命令拥有的本域准备、状态与事务内写入。
mod effect;
mod load;
pub mod mapping;
pub mod state;
pub use effect::EffectiveChangeWrite;
use erp_core::ids::{PurchaseChangeOrderId, PurchaseOrderRevisionId};
use id_generator::next_id;
pub use load::lock_draft_change;

use crate::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseOrder, PurchaseOrderRevision,
};
/// 在原创建时点分配变更 ID；不读取外域或预写审计。
///
/// # 错误
/// 原变更实体校验失败时返回原领域错误。
pub fn new_change(
    order: &PurchaseOrder,
    base_revision: &PurchaseOrderRevision,
    reason: &str,
    actor_id: &str,
) -> crate::Result<PurchaseChangeOrder> {
    Ok(PurchaseChangeOrder::new(
        PurchaseChangeOrderId::new(next_id()),
        PurchaseChangeOrderData {
            purchase_order_id: order.base.id.clone().into(),
            base_revision_id: PurchaseOrderRevisionId::new(base_revision.base.id.clone()),
            reason: reason.to_string(),
        },
        actor_id,
    )?)
}
