//! 采购变更命令拥有的本域准备、状态与事务内写入。
mod draft;
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
/// # 参数
/// * `order` - 来源采购单，只取其稳定 ID
/// * `base_revision` - 冻结的基准采购版本，只取其稳定 ID
/// * `reason` - 变更原因原文
/// * `actor_id` - 创建人
///
/// # 返回
/// 返回新建的草稿变更单。
///
/// # 错误
/// 实体校验失败时，`erp_core::Error` 经 `?` 变为 `Logic`。
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
