//! 时间线复用拥有领域的详情授权，不授予关联单据访问权。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::repository::PurchasePaymentRequestExt;
use erp_identity::{SharedRbacService, subject};
use erp_read_models::finance::payment_request::PurchasePaymentRequestReadService;
use erp_read_models::sales_center::access::SalesAccess;
use erp_read_models::sales_timeline::{SalesTimelineReadService, TimelineAccessPort, TimelineDocument};
use erp_read_models::{Error, Result};
use erp_returns::repository::ReturnsExt;
use erp_workflow::{DocumentType, WorkflowAuthorizationPort};
use mongodb::Database;
use persistence_core::Executor;

use super::workflow::workflow_auth;
use super::{MongoPurchaseDataScope, funds_access_with_rbac, purchase_access};

mod policy;
use policy::ReadPolicy;

struct TimelineAccess {
    db: Database,
    rbac: SharedRbacService,
}

/// 装配销售时间线的跨域授权来源。
///
/// # 参数
/// `db`、`rbac` 为应用的数据库和当前权限服务。
/// # 返回
/// 返回统一的时间线读取服务。
/// # 错误
/// 构造不执行 I/O。
pub fn sales_timeline_service(db: Database, rbac: SharedRbacService) -> SalesTimelineReadService {
    SalesTimelineReadService::new(db.clone(), rbac.clone(), Arc::new(TimelineAccess { db, rbac }))
}

#[async_trait]
impl TimelineAccessPort for TimelineAccess {
    async fn readable(
        &self,
        actor: &AuditActor,
        document: &TimelineDocument,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        match self.read(actor, document, executor).await {
            Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
            result => result,
        }
    }
}

impl TimelineAccess {
    async fn read(
        &self,
        actor: &AuditActor,
        document: &TimelineDocument,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let policy = ReadPolicy::for_document(document.kind);
        let permission = policy.permission(document.kind)?;
        if !self.rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await? {
            return Ok(false);
        }
        match policy {
            ReadPolicy::Invoice => {
                funds_access_with_rbac(self.db.clone(), self.rbac.clone())
                    .source_document_readable(
                        actor,
                        "invoice",
                        &document.id,
                        Some(&purchase_access(self.db.clone(), self.rbac.clone())),
                        executor,
                    )
                    .await
            },
            ReadPolicy::PaymentRequest => self.payment_request(actor, &document.id, executor).await,
            ReadPolicy::SalesReturn => {
                Ok(self.db.sales_return_cases().find_by_id(&document.id, executor).await?.is_some())
            },
            ReadPolicy::PurchaseReturn => self.purchase_return(actor, &document.id, executor).await,
            ReadPolicy::Fulfillment => self.fulfillment(actor, document, executor).await,
            ReadPolicy::ApprovalSource => workflow_auth(self.db.clone(), self.rbac.clone())
                .approval_source_readable(actor, document.kind, &document.id, executor)
                .await
                .map_err(Into::into),
        }
    }

    async fn purchase_return(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(order) = self.db.purchase_return_orders().find_by_id(id, executor).await? else {
            return Ok(false);
        };
        // 与退货详情一致，使用真实来源采购单重新检查 detail 范围。
        purchase_access(self.db.clone(), self.rbac.clone())
            .require_object(actor, "detail", order.purchase_order_id.as_ref(), &[], executor)
            .await?;
        Ok(true)
    }

    async fn payment_request(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(request) = self.db.purchase_payment_requests().find_by_id(id, executor).await? else {
            return Ok(false);
        };
        let service = PurchasePaymentRequestReadService::new(
            self.db.clone(),
            self.rbac.clone(),
            MongoPurchaseDataScope::shared(self.db.clone(), self.rbac.clone()),
        );
        service.timeline_readable(actor, &request, executor).await
    }

    async fn fulfillment(
        &self,
        actor: &AuditActor,
        document: &TimelineDocument,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if document.kind == DocumentType::PurchaseReceipt {
            let Some(id) = &document.purchase_order_id else {
                return Ok(false);
            };
            purchase_access(self.db.clone(), self.rbac.clone())
                .require_object(actor, "detail", id, &[], executor)
                .await?;
        } else {
            let Some(id) = &document.sales_order_id else {
                return Ok(false);
            };
            SalesAccess::new(self.db.clone(), self.rbac.clone())
                .require_object(actor, "detail", id, &[], executor)
                .await?;
        }
        Ok(true)
    }
}
