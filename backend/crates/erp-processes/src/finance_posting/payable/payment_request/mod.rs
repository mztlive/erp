//! 普通采购付款申请：来源、额度、审批与执行在同一事务协调。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt};
use erp_identity::SharedRbacService;
use erp_read_models::finance::payment_request::PurchasePaymentRequestReadService;
use erp_workflow::{ApprovalActionContext, ApprovalObjectReadPort, FailClosedObjectReadPort};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::adapters::MongoPurchaseDataScope;
use crate::adapters::identity::shared_rbac_service;
use crate::procure_to_pay::sync_payment_arrangement;
use crate::{Error, Result};

mod account;
mod basis;
mod batch;
mod cancel;
pub mod credit;
mod current_sales;
mod draft;
pub(crate) mod execution;
mod source;
mod submit;
pub(crate) use cancel::{cancel_approval, reject};
pub(crate) use execution::{approve, restore_reversed_payment};

/// 采购付款申请完整跨领域命令入口。
pub struct PurchasePaymentRequestProcess {
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
}

impl PurchasePaymentRequestProcess {
    /// 绑定本流程数据库与授权实例。
    /// # 参数
    /// * `db` - 应用数据库。
    /// # 返回
    /// 返回尚未装配审批对象读取端口的流程。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { rbac: shared_rbac_service(db.clone()), db, object_read: Arc::new(FailClosedObjectReadPort) }
    }
    /// 注入共享授权实例。
    /// # 参数
    /// * `rbac` - 应用共享 RBAC。
    /// # 返回
    /// 返回装配后的流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = rbac;
        self
    }
    /// 注入审批对象读取端口。
    /// # 参数
    /// * `object_read` - 组合根审批对象资格端口。
    /// # 返回
    /// 返回装配后的流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_object_read(mut self, object_read: Arc<dyn ApprovalObjectReadPort>) -> Self {
        self.object_read = object_read;
        self
    }

    async fn authorized_result(&self, id: &str, actor: &AuditActor) -> Result<PurchasePaymentRequest> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = load(&db, &id, executor).await?;
                    require_sources(&db, &rbac, &request, &actor, "detail", executor).await?;
                    Ok(request)
                })
            })
            .await
    }
}

pub(super) async fn load(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchasePaymentRequest> {
    db.purchase_payment_requests()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款申请不存在".into()))
}

fn ensure_version(request: &PurchasePaymentRequest, version: u64) -> Result<()> {
    if request.base.version != version {
        return Err(Error::ConflictError("付款申请版本已变化，请刷新后重试".into()));
    }
    Ok(())
}

fn business_org(request: &PurchasePaymentRequest) -> Result<&str> {
    request
        .org_unit_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("付款申请缺少真实业务归属部门".into()))
}

fn ensure_creator(request: &PurchasePaymentRequest, actor: &AuditActor) -> Result<()> {
    if request.created_by != actor.id() {
        return Err(Error::Forbidden("只能修改或撤回本人付款申请".into()));
    }
    Ok(())
}

async fn release_remaining(
    db: &Database,
    request: &PurchasePaymentRequest,
    actor: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    for line in &request.lines {
        let remaining = line.remaining();
        if remaining > Amount::zero()
            && !db
                .payable_accounts()
                .release_payment_request(&line.payable_account_id, &remaining, actor, executor)
                .await?
        {
            return Err(Error::ConflictError("付款申请占用金额已变化，请刷新后重试".into()));
        }
        sync_payment_arrangement(db, &line.purchase_order_id, true, executor).await?;
    }
    Ok(())
}

async fn require_sources(
    db: &Database,
    rbac: &SharedRbacService,
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut ids = Vec::with_capacity(request.data.lines.len());
    for line in &request.data.lines {
        let account = db
            .payable_accounts()
            .find_by_id(&line.payable_account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源应付不存在".into()))?;
        ids.push(account.source_document_id);
    }
    let read = PurchasePaymentRequestReadService::new(
        db.clone(),
        rbac.clone(),
        MongoPurchaseDataScope::shared(db.clone(), rbac.clone()),
    );
    read.require_request_sources(actor, action, &ids, executor).await?;
    Ok(())
}

/// 校验回调仍属于当前冻结提交及审批实例。
/// # 参数
/// * `db` / `context` / `executor` - 当前运行时上下文与审批事务。
/// # 返回
/// 当前提交身份一致时成功。
/// # 错误
/// 已结束、旧实例或旧版本的回调拒绝写入。
pub(crate) async fn validate_approval_context(
    db: &Database,
    context: &ApprovalActionContext,
    executor: &mut dyn Executor,
) -> Result<()> {
    let request = load(db, context.business_object_id(), executor).await?;
    if request.status != PurchasePaymentRequestStatus::InApproval
        || request.submission_version.to_string() != context.subject_version()
        || request.approval_instance_id.as_deref() != Some(context.approval_process_instance_id())
    {
        return Err(Error::ConflictError("审批决定不属于当前付款申请提交版本".into()));
    }
    Ok(())
}
