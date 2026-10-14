//! 运费另计跟进：采购标记"供应商不收运费"，采购单访问校验、标记与审计同事务提交。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderLineId;
use erp_procurement::dto::purchase_order::{
    WAIVE_FREIGHT_ACTION, WaivePurchaseFreightRequest, WaivePurchaseFreightResult,
};
use erp_procurement::repository::PurchaseOrderExt;
use persistence_core::Transactional;
use validator::Validate;

use super::PurchaseOrderProcess;
use crate::adapters::purchase_access;
use crate::audit::persist_log;
use crate::{Error, Result};

impl PurchaseOrderProcess {
    /// 标记采购单某商品的运费另计由供应商免收。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单
    /// * `req` - 销售行与原因
    /// * `actor` - 已通过 `purchase_order:update` 鉴权的操作人
    ///
    /// # 返回
    /// 返回已标记的采购单与销售行。
    ///
    /// # 错误
    /// 采购单不可见或不可维护、该行不是运费另计、已标记过或原因不合法时返回错误。
    pub async fn waive_freight(
        &self,
        purchase_order_id: &str,
        req: WaivePurchaseFreightRequest,
        actor: &AuditActor,
    ) -> Result<WaivePurchaseFreightResult> {
        req.validate()?;
        let db = self.db.clone();
        let rbac = self.require_rbac()?.clone();
        let purchase_order_id = purchase_order_id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut order = purchase_access(db.clone(), rbac)
                        .require_object(&actor, "update", &purchase_order_id, &[], executor)
                        .await?;
                    let line = SalesOrderLineId::new(req.sales_order_line_id.trim());
                    order
                        .waive_freight(&line, req.reason.clone(), actor.id(), Instant::now())
                        .map_err(|error| Error::ValidationError(error.to_string()))?;
                    let audit = actor.clone().resource_log_with_message(
                        WAIVE_FREIGHT_ACTION,
                        "purchase_order",
                        order.base.id.clone(),
                        Some(format!("供应商不收运费：{}", req.reason.trim())),
                    )?;
                    db.purchase_orders().update(&mut order, executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(WaivePurchaseFreightResult {
                        purchase_order_id: order.base.id.clone(),
                        sales_order_line_id: line.to_string(),
                    })
                })
            })
            .await
    }
}
