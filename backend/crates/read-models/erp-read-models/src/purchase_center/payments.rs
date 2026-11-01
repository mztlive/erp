//! 采购详情资格与关联付款读取必须在同一事务证明。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::PurchaseOrderId;
use persistence_core::Transactional;

use super::PurchaseOrderReadService;
use crate::Result;
use crate::finance::purchase_payments::{PurchasePaymentView, payment_records};

impl PurchaseOrderReadService {
    /// 读取已核销到当前采购单的付款及回单展示字段。
    /// # 参数
    /// `actor` 为当前账号，`id` 为采购单身份。
    /// # 返回
    /// 按付款时间倒序返回本单有效净核销，合并付款不暴露其他采购分配。
    /// # 错误
    /// 采购详情资格失效、来源不可见或文件事实读取失败时拒绝。
    pub async fn payment_records(&self, actor: &AuditActor, id: &str) -> Result<Vec<PurchasePaymentView>> {
        let this = Self::with_scope(self.db.clone(), Arc::clone(&self.data_scope));
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.access().resolve_detail(&actor, &id, executor).await?;
                    payment_records(&this.db, &PurchaseOrderId::new(id), executor).await
                })
            })
            .await
    }
}
