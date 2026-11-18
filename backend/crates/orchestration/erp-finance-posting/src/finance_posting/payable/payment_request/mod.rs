//! 普通采购付款申请：来源、额度、审批与执行在同一事务协调。

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt};
use erp_identity::entity::authorization::{ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, purchase_payment_request};
use erp_procure_to_pay::procure_to_pay::sync_payment_arrangement;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::ApprovalActionContext;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, FinancePostingPorts, Result};

mod account;
mod basis;
mod batch;
mod cancel;
pub mod credit;
mod current_sales;
mod deposit_execution;
mod draft;
pub(crate) mod execution;
mod source;
mod submit;
pub use cancel::{cancel_approval, reject};
pub use execution::{approve, restore_reversed_payment, sync_task};

/// 采购付款申请完整跨领域命令入口。
pub struct PurchasePaymentRequestProcess {
    db: Database,
    rbac: FinancePostingPorts,
}

impl PurchasePaymentRequestProcess {
    /// 绑定本流程数据库与授权实例。
    /// # 参数
    /// * `db` - 应用数据库。
    /// # 返回
    /// 返回绑定共享财务业务端口的流程。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: FinancePostingPorts) -> Self {
        Self { rbac, db }
    }
}

/// 重放和写入均先证明原动作资格，再读取完整来源与当前结果。
async fn authorized_request(
    db: &Database,
    authorization: &AuthorizationService,
    id: &str,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<PurchasePaymentRequest> {
    let permission = match action {
        "submit" => purchase_payment_request::SUBMIT,
        "withdraw" => purchase_payment_request::WITHDRAW,
        "close" => purchase_payment_request::CLOSE,
        _ => return Err(Error::ValidationError("未知付款申请命令".into())),
    };
    authorization.require(actor, &permission, AccessTarget::record(id)?, executor).await?;
    load(db, id, executor).await
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
    rbac: &FinancePostingPorts,
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseOrder>> {
    let mut ids = Vec::with_capacity(request.data.lines.len());
    for line in &request.data.lines {
        let account = db
            .payable_accounts()
            .find_by_id(&line.payable_account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源应付不存在".into()))?;
        ids.push(account.source_document_id);
    }

    let (permission, target) = request_target(request, action, &ids)?;
    rbac.authorization.require(actor, &permission, target, executor).await?;

    let mut orders = Vec::new();
    for id in ids {
        orders.push(
            db.purchase_orders()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("付款申请采购来源不存在".into()))?,
        );
    }
    Ok(orders)
}

/// 只构造服务端完整来源候选；授权规则始终由 identity 选择。
fn request_target(
    request: &PurchasePaymentRequest,
    action: &str,
    ids: &[String],
) -> Result<(Permission, AccessTarget)> {
    let permission = match action {
        "create" => purchase_payment_request::CREATE,
        "update" => purchase_payment_request::UPDATE,
        "detail" => purchase_payment_request::DETAIL,
        "submit" => purchase_payment_request::SUBMIT,
        "withdraw" => purchase_payment_request::WITHDRAW,
        "close" => purchase_payment_request::CLOSE,
        _ => return Err(Error::ValidationError("未知付款申请动作".into())),
    };
    let candidate = ObjectFacts {
        resource: "purchase_payment_request".into(),
        relations: [(
            RelationKey::parse("purchase_source")?,
            RelatedObjects { expected_ids: ids.iter().cloned().collect(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    };
    let target = match action {
        "create" => AccessTarget::Candidate(candidate),
        "update" => AccessTarget::OwnershipChange { id: request.base.id.to_string(), candidate },
        _ => AccessTarget::record(&request.base.id.to_string())?,
    };
    Ok((permission, target))
}

/// 校验回调仍属于当前冻结提交及审批实例。
/// # 参数
/// * `db` / `context` / `executor` - 当前运行时上下文与审批事务。
/// # 返回
/// 当前提交身份一致时成功。
/// # 错误
/// 已结束、旧实例或旧版本的回调拒绝写入。
pub async fn validate_approval_context(
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

#[cfg(test)]
mod command_access_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 提交、撤回和关闭均须先取得原动作授权；取证失败不得连接单据仓储。
    #[tokio::test]
    async fn command_access_stops_before_document_read_when_authorization_fails() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap()
            .database("payment_request_command_access");
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for action in ["submit", "withdraw", "close"] {
            let error = authorized_request(
                &db,
                &authorization,
                "request",
                &actor,
                action,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }

        let error = authorized_request(
            &db,
            &authorization,
            "request",
            &actor,
            "detail",
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message == "未知付款申请命令"));
    }
}
