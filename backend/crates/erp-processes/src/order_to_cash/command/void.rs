use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::SalesOrderId;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::VoidSalesOrderRequest;
use erp_sales::entity::sales_order::WorkingPurpose;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 作废销售单草稿（主状态 `DRAFT → VOIDED`；放弃有效工作副本）。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后的销售单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    #[tracing::instrument(
        name = "sales_order.void",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "void")
    )]
    pub async fn void_sales_order(
        &self,
        id: &str,
        req: VoidSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let access = self.command_access(actor, "delete")?;
        let authorized_order = access.current(id, &mut NoTransaction).await?;
        let mut order = authorized_order;
        let expected_order_version = order.base.version;
        if !order.matches_version(req.version) {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        order.void(actor.id())?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        let audit = actor.clone().resource_log("sales_order.void", "sales_order", id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access.revalidate(&order.base.id, expected_order_version, session).await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .persist_void(&mut order, working_copy.as_mut(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await?;

        self.read_model().sales_order_detail(id, None).await.map_err(crate::Error::from)
    }
}
