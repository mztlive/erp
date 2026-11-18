use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use erp_identity::AuthorizationService;
use erp_permissions::sales_order;
use erp_read_models::sales_center::access::SalesAccess;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::VoidSalesOrderRequest;
use erp_sales::entity::sales_order::WorkingPurpose;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::SalesOrderService;
use erp_workflow::service::work_item::approval_rework::complete_document_rework;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapter::sales_approval_ports;
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
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor_owned = actor.clone();
        let order_id = id.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    void_order(&db, authorization, &order_id, req.version, &actor_owned, executor).await
                })
            })
            .await?;

        self.command_result(id, actor, &sales_order::VOID).await
    }
}

/// 在原快照先授权，再读取当前版本、作废工作副本并原子持久化。
async fn void_order(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    id: &str,
    expected_version: u64,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut order = SalesAccess::new(db.clone(), authorization)
        .require_object(actor, &sales_order::VOID, id, executor)
        .await?;
    if !order.matches_version(expected_version) {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".into()));
    }

    order.void(actor.id())?;
    let order_id = SalesOrderId::new(order.base.id.clone());
    let mut working_copy = db
        .sales_order_working_copies()
        .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, executor)
        .await?;
    if let Some(copy) = &mut working_copy {
        copy.abandon()?;
    }

    let audit = actor.clone().resource_log("sales_order.void", "sales_order", id.to_string())?;
    SalesOrderService::new(db.clone()).persist_void(&mut order, working_copy.as_mut(), executor).await?;
    complete_document_rework(
        db,
        sales_approval_ports(order.business_type)?.document_type.as_str(),
        &order.base.id,
        &order.stable.updated_by,
        Instant::now(),
        executor,
    )
    .await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn void_stops_at_authorization_before_loading_version_or_state() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("void_authorization");
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error = void_order(
            &db,
            unavailable_authorization(),
            "order",
            0,
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
