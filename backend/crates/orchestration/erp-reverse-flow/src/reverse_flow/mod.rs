//! 退货、退款和资金冲正的跨域事务、审批与审计组合入口。
use application_core::AuditActor;
use erp_workflow::service::approval::policy::ApprovalDomainAction;

use crate::ReverseFlowPorts;

mod adapter;
mod approval_outcome;
use approval_outcome::{ApprovalOutcome, persist_approval_outcome};
mod cancel_approval;
mod cancel_write;
pub(crate) mod command_recovery;
mod commit_access;
mod customer_refund;
mod direct_return;
mod draft_update;
mod payment_posting;
mod payment_reversal;
mod purchase_return;
mod receipt_reversal;
mod result;
mod return_settlement;
mod sales_return;
mod start_approval;
mod supplier_refund;

pub use adapter::{
    customer_refund_object_readable, payment_reversal_object_readable, receipt_reversal_object_readable,
    supplier_refund_object_readable,
};
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
pub use payment_posting::PaymentReversalProcess;
use persistence_core::Executor;
pub use receipt_reversal::ReceiptReversalProcess;

use crate::{Error, Result};

/// 退货、退款和冲正命令的原事务与审批授权组合。
#[derive(Clone)]
pub struct ReturnsProcess {
    db: Database,
    rbac: ReverseFlowPorts,
}
impl ReturnsProcess {
    /// 使用共享业务端口构造命令入口。
    ///
    /// # 参数
    /// * `db` - 组合根数据库。
    ///
    /// # 返回
    /// 返回使用给定业务端口的命令入口。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: ReverseFlowPorts) -> Self {
        Self { db, rbac }
    }
    /// 用当前数据库构造退货退款领域服务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库的 `ReturnsService`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn domain(&self) -> ReturnsService {
        ReturnsService::new(self.db.clone())
    }
}

/// 在审批运行时的外层事务内执行客户退款或供应商退款的最终过账。
///
/// 只接受 `DocumentType::CustomerRefund` 与 `DocumentType::SupplierRefund`。回款冲正和付款冲正不在此分派。
///
/// # 参数
/// * `db` - 数据库实例
/// * `document_type` - 退款单据类型
/// * `business_object_id` - 业务对象 ID
/// * `actor` - 已认证操作人
/// * `executor` - 审批运行时持有的执行器
///
/// # 返回
/// 领域过账、业务审计与全部关联事实写入成功时返回 `Ok(())`。
///
/// # 错误
/// 单据类型不是客户退款或供应商退款时返回 `BusinessLogicError`。领域状态、额度或持久化失败时返回对应错误。
pub async fn finalize_approved_return(
    db: &Database,
    rbac: &ReverseFlowPorts,
    document_type: DocumentType,
    business_object_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    match document_type {
        DocumentType::CustomerRefund => {
            customer_refund::apply_customer_refund_final_post(db, business_object_id, actor, executor).await
        },
        DocumentType::SupplierRefund => {
            supplier_refund::apply_supplier_refund_final_post(db, rbac, business_object_id, actor, executor)
                .await
        },
        other => Err(Error::BusinessLogicError(format!(
            "{}不能办理此项退款或冲正终审操作，请核对当前单据",
            other.label()
        ))),
    }
}

/// 在审批运行时持有的事务内撤回资金纠错单审批。
///
/// # 参数
/// * `db` - 数据库。
/// * `document_type` - 客户退款、供应商退款、回款冲正或付款冲正。
/// * `id` - 单据主键。
/// * `action` - 合同强类型领域动作。
/// * `actor` - 已认证操作人。
/// * `executor` - 审批运行时持有的执行器。
///
/// # 返回
/// 成功时无返回值。单据已按动作写回，并追加撤回审计。
///
/// # 错误
/// 单据不存在、类型与动作不匹配、状态迁移或 CAS 写入失败时返回错误。
pub async fn cancel_approval(
    db: &Database,
    document_type: DocumentType,
    id: &str,
    action: ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_approval_outcome(
        db,
        ApprovalOutcome {
            document_type,
            id,
            action,
            expected_subject_version: None,
            audit_action: "returns.cancel_approval",
        },
        actor,
        executor,
    )
    .await
}

/// 同事务驳回退款与冲正单；资金事实仅由最终通过产生，驳回不反向写正式事实。
/// # 参数
/// * `db` - 数据库。
/// * `document_type` / `id` - 原单身份。
/// * `expected_subject_version` - 被驳回实例冻结的审批主题版本。
/// * `action` - 对应的驳回领域动作。
/// * `actor` - 已授权审批人。
/// * `executor` - 审批运行时事务。
/// # 返回
/// 保存已驳回状态和审计。
/// # 错误
/// 非对应驳回动作、单据不存在或状态冲突时拒绝。
pub async fn reject_approval(
    db: &Database,
    document_type: DocumentType,
    id: &str,
    expected_subject_version: u32,
    action: ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::Internal("退款冲正驳回缺少运行时事务".into()));
    }

    let expected = match document_type {
        DocumentType::CustomerRefund => ApprovalDomainAction::CustomerRefundReject,
        DocumentType::SupplierRefund => ApprovalDomainAction::SupplierRefundReject,
        DocumentType::ReceiptReversal => ApprovalDomainAction::ReceiptReversalReject,
        DocumentType::PaymentReversal => ApprovalDomainAction::PaymentReversalReject,
        _ => return Err(Error::ValidationError("单据不支持退款冲正驳回".into())),
    };
    if action != expected {
        return Err(Error::ValidationError("单据与驳回动作不匹配".into()));
    }

    persist_approval_outcome(
        db,
        ApprovalOutcome {
            document_type,
            id,
            action,
            expected_subject_version: Some(expected_subject_version),
            audit_action: "returns.reject_approval",
        },
        actor,
        executor,
    )
    .await
}

#[cfg(test)]
mod boundary_tests {
    use erp_core::AccountKind;
    use mongodb::Client;
    use persistence_core::ReadExecutor;

    use super::*;

    /// 运行时写步骤拒绝普通读取执行器，且检查本身保留原读取阶段。
    #[tokio::test]
    async fn rejection_requires_snapshot_without_revoking_read_phase() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let mut executor = ReadExecutor::default();
        let boundary = executor.boundary().unwrap();

        let result = reject_approval(
            &db,
            DocumentType::CustomerRefund,
            "refund",
            1,
            ApprovalDomainAction::CustomerRefundReject,
            &actor,
            &mut executor,
        )
        .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message.contains("运行时事务")));
        assert!(boundary.is_current(&executor));
    }
}

#[cfg(test)]
pub(super) mod testing {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_approval_runtime::ApprovalRuntime;
    use erp_core::AccountKind;
    use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
    use erp_identity::testing::unavailable_authorization;
    use erp_identity::{RbacService, Result as IdentityResult};
    use erp_workflow::entity::work_item::{FinanceResponsibilityOperation, WorkItemType};
    use erp_workflow::ports::{FailClosedAuditPort, FinanceResponsibilityPort};
    use erp_workflow::service::work_item::ResolvedFinanceResponsibility;
    use erp_workflow::{Result as WorkflowResult, WorkflowAuthorizationPort};
    use mongodb::Client;

    use super::*;

    #[derive(Clone)]
    struct NoAuth;
    #[async_trait]
    impl WorkflowAuthorizationPort for NoAuth {}

    struct NoIdentityAudit;
    #[async_trait]
    impl IdentityAuditPort for NoIdentityAudit {
        fn resource_log(
            &self,
            _: AuditActor,
            _: &str,
            _: &str,
            _: String,
        ) -> IdentityResult<PreparedResourceAudit> {
            panic!("拒绝的撤回不得构造身份审计")
        }
        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
            panic!("拒绝的撤回不得写入身份审计")
        }
    }

    struct NoFinance;
    #[async_trait]
    impl FinanceResponsibilityPort for NoFinance {
        async fn resolve_finance_responsibility(
            &self,
            _: FinanceResponsibilityOperation,
            _: &str,
            _: &mut dyn Executor,
        ) -> WorkflowResult<ResolvedFinanceResponsibility> {
            panic!("拒绝的撤回不得分配财务责任")
        }
        async fn resolve_return_finance_responsibility(
            &self,
            _: WorkItemType,
            _: &str,
            _: &mut dyn Executor,
        ) -> WorkflowResult<ResolvedFinanceResponsibility> {
            panic!("拒绝的撤回不得分配退货责任")
        }
    }

    pub(super) async fn process() -> ReturnsProcess {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("reverse_cancel_authorization");
        let identity = unavailable_authorization();
        let rbac = Arc::new(RbacService::new(db.clone(), Arc::new(NoIdentityAudit)));
        let runtime = Arc::new(ApprovalRuntime::new(
            db.clone(),
            identity.clone(),
            rbac,
            NoAuth,
            Arc::new(FailClosedAuditPort),
        ));
        ReturnsProcess::new(db.clone(), ReverseFlowPorts::new(identity, runtime, Arc::new(NoFinance)))
    }

    pub(super) fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
    }

    pub(super) fn denied<T>(result: Result<T>) {
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }
}
