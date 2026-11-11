//! 退款提交和回放使用真实资金来源授权，不依赖尚未生成的审批任务。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::ReceiptReversalId;
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission};
use erp_returns::repository::ReturnsExt;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::ReturnsProcess;
use crate::{Error, Result, ReverseFlowPorts};

#[async_trait]
trait ReplayAuthorizationPort: Send + Sync {
    async fn qualification(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> Result<SubmissionQualification>;
    async fn source_readable(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn ensure_initiator(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// 同一次账号与权限取证的提交资格，保留现有两种拒绝语义。
#[derive(Clone, Copy)]
enum SubmissionQualification {
    Inactive,
    MissingPermission,
    Allowed,
}

struct MongoReplayAuthorization<'a> {
    db: &'a Database,
    rbac: &'a ReverseFlowPorts,
}
#[async_trait]
impl ReplayAuthorizationPort for MongoReplayAuthorization<'_> {
    async fn qualification(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> Result<SubmissionQualification> {
        let authorization = AuthorizationService::new(
            self.db.clone(),
            self.rbac.identity.clone(),
            AuthorizationRegistry::default(),
        );
        let required = [Permission::parse(permission)?];
        Ok(match authorization.permissions(actor, &required, executor).await? {
            None => SubmissionQualification::Inactive,
            Some(qualified) if qualified.same_role(&required) => SubmissionQualification::Allowed,
            Some(_) => SubmissionQualification::MissingPermission,
        })
    }
    async fn source_readable(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.rbac.runtime.source_readable(actor, kind, id, executor).await?)
    }
    async fn ensure_initiator(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_financial_initiator(self.db, actor, kind, id, executor).await
    }
}
/// 账号和提交资格必须先于具体来源读取被拒绝。
async fn ensure_qualification<P: ReplayAuthorizationPort>(
    port: &P,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    match port.qualification(actor, permission, executor).await? {
        SubmissionQualification::Inactive => Err(Error::Forbidden("当前账号不可提交该退款或冲正单".into())),
        SubmissionQualification::MissingPermission => {
            Err(Error::Forbidden("当前账号缺少退款提交权限".into()))
        },
        SubmissionQualification::Allowed => Ok(()),
    }
}
/// 同一事务重验静态提交能力和完整资金来源。
async fn authorize<P: ReplayAuthorizationPort>(
    port: &P,
    actor: &AuditActor,
    kind: DocumentType,
    permission: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_qualification(port, actor, permission, executor).await?;
    authorize_source(port, actor, kind, id, executor).await
}

/// 当前阶段已证明提交资格后，检查完整资金来源与原经办职责。
async fn authorize_source<P: ReplayAuthorizationPort>(
    port: &P,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !port.source_readable(actor, kind, id, executor).await? {
        return Err(Error::Forbidden("无权读取退款所引用的完整资金来源".to_string()));
    }
    port.ensure_initiator(actor, kind, id, executor).await
}

/// 回放重读原经办职责，资格不依赖已提交单据当前是否仍为草稿。
async fn ensure_financial_initiator(
    db: &Database,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = ReturnsService::new(db.clone());
    match kind {
        DocumentType::CustomerRefund => {
            service.load_customer_refund(id, executor).await?.ensure_submitter(actor.id())?
        },
        DocumentType::SupplierRefund => {
            service.load_supplier_refund(id, executor).await?.ensure_submitter(actor.id())?
        },
        DocumentType::ReceiptReversal => db
            .receipt_reversals()
            .find_by_id(&ReceiptReversalId::new(id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?
            .ensure_submitter(actor.id())?,
        DocumentType::PaymentReversal => {
            service.load_payment_reversal(id, executor).await?.ensure_submitter(actor.id())?
        },
        _ => {},
    }
    Ok(())
}

/// 在读取具体退款或冲正来源前一次确认账号及提交资格。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 共享 RBAC。
/// * `actor` - 待重验的操作人。
/// * `permission` - 服务端固定的提交动作权限码。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 账号和提交资格仍有效时无返回值。
///
/// # 错误
/// 账号失效或缺少提交权限时返回 `Forbidden`。授权查询失败时返回对应错误。
pub(in crate::reverse_flow) async fn ensure_submit_permission(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_qualification(&MongoReplayAuthorization { db, rbac }, actor, permission, executor).await
}

/// 已在同一执行器证明提交资格后，重验资金来源与原经办职责。
/// # 参数
/// db、rbac 为共享设施，actor 为当前账号，kind、id 为精确单据，executor 为原事务。
/// # 返回
/// 完整来源可读且原经办职责成立时成功。
/// # 错误
/// 来源无权、经办人不符或仓储读取失败时返回对应错误。
pub(in crate::reverse_flow) async fn ensure_source_authorized(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize_source(&MongoReplayAuthorization { db, rbac }, actor, kind, id, executor).await
}

/// 在同一事务内重验账号、提交权限、资金来源可读性和原经办职责。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 共享 RBAC。
/// * `actor` - 待重验的操作人。
/// * `kind` - 退款或冲正单据类型。
/// * `permission` - 提交动作权限码。
/// * `id` - 单据主键。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 授权通过时无返回值。
///
/// # 错误
/// 账号失效、缺少提交权限、无权读取资金来源、经办人不符或仓储失败时返回对应错误。
pub(in crate::reverse_flow) async fn ensure_replay_authorized(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    kind: DocumentType,
    permission: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize(&MongoReplayAuthorization { db, rbac }, actor, kind, permission, id, executor).await
}

impl ReturnsProcess {
    /// 已提交命令回放仍在新事务内重验当前职责和资金来源，保持零写入。
    ///
    /// # 参数
    /// * `kind` - 退款或冲正单据类型。
    /// * `permission` - 提交动作权限码。
    /// * `id` - 已提交单据主键。
    /// * `actor` - 回放操作人。
    ///
    /// # 返回
    /// 重验通过时无返回值。不写入业务数据。
    ///
    /// # 错误
    /// 事务内授权失败或事务本身失败时返回对应错误。
    pub(in crate::reverse_flow) async fn authorize_refund_replay(
        &self,
        kind: DocumentType,
        permission: &str,
        id: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let permission = permission.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    ensure_replay_authorized(&db, &rbac, &actor, kind, &permission, &id, executor).await
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::NoTransaction;

    use super::*;

    struct RecordingPort {
        qualification: SubmissionQualification,
        qualification_error: bool,
        source: bool,
        initiator: bool,
        kind: DocumentType,
        permission: &'static str,
        calls: Mutex<Vec<&'static str>>,
    }
    #[async_trait]
    impl ReplayAuthorizationPort for RecordingPort {
        async fn qualification(
            &self,
            _: &AuditActor,
            permission: &str,
            _: &mut dyn Executor,
        ) -> Result<SubmissionQualification> {
            assert_eq!(permission, self.permission);
            self.calls.lock().unwrap().push("qualification");
            if self.qualification_error {
                return Err(Error::Internal("identity unavailable".into()));
            }
            Ok(self.qualification)
        }
        async fn source_readable(
            &self,
            _: &AuditActor,
            kind: DocumentType,
            id: &str,
            _: &mut dyn Executor,
        ) -> Result<bool> {
            assert_eq!((kind, id), (self.kind, "document-1"));
            self.calls.lock().unwrap().push("source");
            Ok(self.source)
        }
        async fn ensure_initiator(
            &self,
            _: &AuditActor,
            kind: DocumentType,
            id: &str,
            _: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!((kind, id), (self.kind, "document-1"));
            self.calls.lock().unwrap().push("initiator");
            if !self.initiator {
                return Err(Error::Forbidden("仅原经办人可以提交".into()));
            }
            Ok(())
        }
    }
    /// 首次提交与回放仅依赖自己的静态动作和精确资金来源，无审批参与权限依赖。
    #[tokio::test]
    async fn submission_checks_active_actor_action_and_exact_source() {
        for (kind, permission) in [
            (DocumentType::CustomerRefund, "customer_refund:submit"),
            (DocumentType::SupplierRefund, "supplier_refund:submit"),
            (DocumentType::ReceiptReversal, "receipt_reversal:submit"),
            (DocumentType::PaymentReversal, "payment_reversal:submit"),
        ] {
            for (qualification, source, initiator, expected_error, expected_calls) in [
                (
                    SubmissionQualification::Allowed,
                    true,
                    true,
                    None,
                    vec!["qualification", "source", "initiator"],
                ),
                (
                    SubmissionQualification::Inactive,
                    true,
                    true,
                    Some("当前账号不可提交该退款或冲正单"),
                    vec!["qualification"],
                ),
                (
                    SubmissionQualification::MissingPermission,
                    true,
                    true,
                    Some("当前账号缺少退款提交权限"),
                    vec!["qualification"],
                ),
                (
                    SubmissionQualification::Allowed,
                    false,
                    true,
                    Some("无权读取退款所引用的完整资金来源"),
                    vec!["qualification", "source"],
                ),
                (
                    SubmissionQualification::Allowed,
                    true,
                    false,
                    Some("仅原经办人可以提交"),
                    vec!["qualification", "source", "initiator"],
                ),
            ] {
                let port = RecordingPort {
                    qualification,
                    qualification_error: false,
                    source,
                    initiator,
                    kind,
                    permission,
                    calls: Mutex::new(Vec::new()),
                };
                let actor = AuditActor::new("actor-1".into(), "actor".into(), erp_core::AccountKind::Admin);
                let result =
                    authorize(&port, &actor, kind, permission, "document-1", &mut NoTransaction).await;
                if let Some(expected) = expected_error {
                    assert!(matches!(result, Err(Error::Forbidden(message)) if message == expected));
                } else {
                    result.unwrap();
                }
                assert_eq!(port.calls.into_inner().unwrap(), expected_calls);
            }
        }
    }

    /// 资格设施错误不得降级为无权，也不得继续读取资金来源。
    #[tokio::test]
    async fn qualification_failure_stops_before_source_reads() {
        let port = RecordingPort {
            qualification: SubmissionQualification::Allowed,
            qualification_error: true,
            source: true,
            initiator: true,
            kind: DocumentType::CustomerRefund,
            permission: "customer_refund:submit",
            calls: Mutex::new(Vec::new()),
        };
        let actor = AuditActor::new("actor-1".into(), "actor".into(), erp_core::AccountKind::Admin);
        let result = authorize(
            &port,
            &actor,
            DocumentType::CustomerRefund,
            "customer_refund:submit",
            "document-1",
            &mut NoTransaction,
        )
        .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "identity unavailable"));
        assert_eq!(port.calls.into_inner().unwrap(), ["qualification"]);
    }
}
