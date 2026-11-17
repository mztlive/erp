//! 时间线复用拥有领域的详情授权，不授予关联单据访问权。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::repository::PurchasePaymentRequestExt;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_permissions::{purchase_order, sales_order};
use erp_read_models::finance::payment_request::PurchasePaymentRequestReadService;
use erp_read_models::sales_center::access::SalesAccess;
use erp_read_models::sales_timeline::{SalesTimelineReadService, TimelineAccessPort, TimelineDocument};
use erp_read_models::{Error, Result};
use erp_returns::repository::ReturnsExt;
use erp_workflow::{DocumentType, WorkflowAuthorizationPort};
use mongodb::Database;
use persistence_core::Executor;

use super::workflow::workflow_auth;
use super::{funds_access, purchase_access};

mod policy;
use policy::ReadPolicy;

struct TimelineAccess {
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
}

/// 装配销售时间线的跨域授权来源。
///
/// # 参数
/// db、rbac 为数据库与身份设施，authorization 为进程共享统一授权服务。
/// # 返回
/// 返回统一的时间线读取服务。
/// # 错误
/// 构造不执行 I/O。
pub fn sales_timeline_service(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> SalesTimelineReadService {
    SalesTimelineReadService::new(
        db.clone(),
        authorization.clone(),
        Arc::new(TimelineAccess { db, rbac, authorization }),
    )
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
        let permission = policy.permission(document.kind);
        if !self.authorization.has_permission(actor, &permission, executor).await? {
            return Ok(false);
        }
        match policy {
            ReadPolicy::Invoice => {
                funds_access(self.db.clone(), self.authorization.clone())
                    .source_document_readable(actor, "invoice", &document.id, executor)
                    .await
            },
            ReadPolicy::PaymentRequest => self.payment_request(actor, &document.id, executor).await,
            ReadPolicy::SalesReturn => {
                Ok(self.db.sales_return_cases().find_by_id(&document.id, executor).await?.is_some())
            },
            ReadPolicy::PurchaseReturn => self.purchase_return(actor, &document.id, executor).await,
            ReadPolicy::Fulfillment => self.fulfillment(actor, document, executor).await,
            ReadPolicy::ApprovalSource => {
                workflow_auth(self.db.clone(), self.rbac.clone(), self.authorization.clone())
                    .approval_source_readable(actor, document.kind, &document.id, executor)
                    .await
                    .map_err(Into::into)
            },
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
        purchase_access(self.db.clone(), self.authorization.clone())
            .require_object(actor, &purchase_order::DETAIL, order.purchase_order_id.as_ref(), executor)
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
        let service = PurchasePaymentRequestReadService::new(self.db.clone(), self.authorization.clone());
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
            purchase_access(self.db.clone(), self.authorization.clone())
                .require_object(actor, &purchase_order::DETAIL, id, executor)
                .await?;
        } else {
            let Some(id) = &document.sales_order_id else {
                return Ok(false);
            };
            SalesAccess::new(self.db.clone(), self.authorization.clone())
                .require_object(actor, &sales_order::DETAIL, id, executor)
                .await?;
        }
        Ok(true)
    }
}
