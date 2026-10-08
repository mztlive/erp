//! 原回款提交的完整载荷收据、当前资格回放与原子启动事务。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_finance::dto::receivable::SubmitCustomerReceiptRequest;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::repository::ReceivableExt;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::SharedRbacService;
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_workflow::service::approval::execution::{PreparedExecution, command_recovery_delay};
use erp_workflow::service::document_registry::find_approval_binding;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::ReceivableProcess;
use super::adapter::{
    self, RECENT_HISTORY_LIMIT, build_customer_receipt_snapshot, customer_receipt_object_readable,
    customer_receipt_responsible_org_id, customer_receipt_start_command, customer_receipt_subject_ref,
    require_frozen_binding, start_approval_command_kind,
};
use super::customer_receipt_command::{
    MongoReceiptCommandWrites, ReceiptReplayPort, ensure_committed_resource, persist_receipt_command_success,
    receipt_command_context, replay_receipt_command,
};
use super::customer_receipt_posting::prepare_dispatch_start;
use super::draft_read::{ensure_receipt_owner, receipt_view_with_actions};
use super::draft_update::ensure_receipt_edit_authorized;
use super::start_approval::{CustomerReceiptStartPersistInput, persist_customer_receipt_start_apply};
use crate::{Error, Result};

/// 原回款主键、版本、幂等键与完整拟核销分配共同构成提交载荷。
///
/// # 参数
/// * `id` - 原回款主键。
/// * `req` - 提交请求，含幂等键与拟核销分配。
/// * `actor_id` - 提交人 ID。
///
/// # 返回
/// 返回绑定该原单与完整请求载荷的命令回执。
///
/// # 错误
/// 幂等键为空或载荷序列化失败时返回错误。
pub(super) fn submit_receipt(
    id: &str,
    req: &SubmitCustomerReceiptRequest,
    actor_id: &str,
) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_payload(
        "customer-receipt-submit-",
        actor_id,
        "customer_receipt.submit",
        "customer_receipt",
        &req.idempotency_key,
        &(id, req),
    )?)
}

impl ReceivableProcess {
    /// 按原顺序冻结提交快照和绑定启动计划，写入留在调用方事务。
    ///
    /// # 参数
    /// * `id` - 原回款主键。
    /// * `receipt` - 已装载的回款单。
    /// * `idempotency_key` - 本次提交幂等键。
    /// * `actor` - 当前提交人。
    /// * `adapter` - 客户回款适配器，提供责任角色。
    ///
    /// # 返回
    /// 返回尚未落库的启动持久化输入。
    ///
    /// # 错误
    /// 主体引用、绑定缺失、快照、责任组织或启动计划准备失败时返回错误。
    pub(super) async fn prepare_receipt_submit_input(
        &self,
        id: &str,
        receipt: CustomerReceipt,
        idempotency_key: &str,
        actor: &AuditActor,
        adapter: adapter::CustomerReceiptAdapter,
    ) -> Result<CustomerReceiptStartPersistInput> {
        let subject = customer_receipt_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = erp_core::common::time::Instant::now();
        let snapshot_payload = build_customer_receipt_snapshot(&receipt, actor.id(), now)?;
        let start =
            customer_receipt_start_command(id, receipt.approval_subject_version, actor.id(), idempotency_key);
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
        let _ = customer_receipt_object_readable(&organization_id, actor.id())?;
        let prepared = prepare_dispatch_start(
            &self.db,
            &binding,
            &subject,
            receipt.approval_subject_version,
            &organization_id,
            actor.id(),
            idempotency_key,
            now,
        )
        .await?;
        Ok(CustomerReceiptStartPersistInput {
            receipt,
            actor: actor.clone(),
            id: id.to_owned(),
            snapshot_payload,
            prepared,
            owner_role: adapter.owner_role,
            organization_id,
            now,
        })
    }

    /// 在状态和版本门禁前，以当前身份、完整来源和原登记人回放同载荷提交。
    ///
    /// # 参数
    /// * `id` - 原回款主键。
    /// * `command` - 原提交命令回执。
    /// * `actor` - 当前已认证操作人。
    ///
    /// # 返回
    /// 命中同一原单的已提交回执时返回当前视图；没有回执时返回 `None`。
    ///
    /// # 错误
    /// 当前资格不足、回执与原单不一致或读取失败时返回错误。
    pub(super) async fn replay_customer_receipt_submit(
        &self,
        id: &str,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<CustomerReceiptView>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let command = command.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let port = MongoReceiptSubmitReplay { db: &db, rbac: &rbac, actor: &actor };
                    replay_authorized_submit(&port, &id, &command, executor).await
                })
            })
            .await
    }

    /// 提交结果未知时只用完整业务收据恢复，每次 fresh 快照都重验当前资格。
    ///
    /// # 参数
    /// * `id` - 原回款主键。
    /// * `command` - 原提交命令回执。
    /// * `actor` - 当前已认证操作人。
    /// * `original_error` - 事务首次返回的错误。
    ///
    /// # 返回
    /// 查证到原提交结果时返回当前视图。
    ///
    /// # 错误
    /// 未命中时返回 `original_error`。原错误为未知提交时，查证失败也保留该错误；
    /// 查证得到不可能已提交的明确错误时返回该错误。
    pub(super) async fn recover_customer_receipt_submit(
        &self,
        id: &str,
        command: &CommandReceipt,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<CustomerReceiptView> {
        recover_authorized_submit(&MongoReceiptSubmitRecovery(self), id, command, actor, original_error).await
    }
}

/// 每次恢复查证由原用例取得独立的只读事务快照。
#[async_trait]
trait ReceiptSubmitRecoveryPort: Send + Sync {
    type Output: Send;

    /// 在独立只读快照中回放原提交；没有命中回执时返回 `None`。
    async fn probe(
        &self,
        id: &str,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<Self::Output>>;
}

struct MongoReceiptSubmitRecovery<'a>(&'a ReceivableProcess);

#[async_trait]
impl ReceiptSubmitRecoveryPort for MongoReceiptSubmitRecovery<'_> {
    type Output = CustomerReceiptView;

    async fn probe(
        &self,
        id: &str,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<CustomerReceiptView>> {
        self.0.replay_customer_receipt_submit(id, command, actor).await
    }
}

/// 有界查证只读原命令；未知提交下资格、回执或视图读取失败均保留原错误。
async fn recover_authorized_submit<P: ReceiptSubmitRecoveryPort>(
    port: &P,
    id: &str,
    command: &CommandReceipt,
    actor: &AuditActor,
    original_error: Error,
) -> Result<P::Output> {
    const RECOVERY_ATTEMPTS: usize = 8;
    for attempt in 0..RECOVERY_ATTEMPTS {
        match port.probe(id, command, actor).await {
            Ok(Some(view)) => return Ok(view),
            Ok(None) => {},
            Err(_) if matches!(original_error, Error::OutcomeUnknown(_)) => return Err(original_error),
            Err(error) if error.command_may_have_committed() => {},
            Err(error) => return Err(error),
        }
        if attempt + 1 < RECOVERY_ATTEMPTS {
            tokio::time::sleep(command_recovery_delay(attempt)).await;
        }
    }
    Err(original_error)
}

/// 启动事实、完整提交收据和成功审计同事务写入；竞争重试只回读既有原单。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 授权源。
/// * `input` - 事务外已冻结的启动输入。
/// * `command` - 原提交命令回执。
///
/// # 返回
/// 返回已提交的原回款单；同载荷已提交时回读该原单。
///
/// # 错误
/// 资格不足、原单不存在、版本变化、历史启动缺少完整收据或事务写入失败时返回错误。
pub(super) async fn persist_receipt_submit(
    db: &Database,
    rbac: &SharedRbacService,
    input: CustomerReceiptStartPersistInput,
    command: CommandReceipt,
) -> Result<CustomerReceipt> {
    let db = db.clone();
    let rbac = rbac.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                let current = authorized_receipt(&db, &rbac, &input.actor, &input.id, executor).await?;
                if committed_submit(&db, &command, &input.id, executor).await? {
                    return Ok(current);
                }
                if !matches!(&input.prepared, PreparedExecution::Apply(_)) {
                    return Err(Error::ConflictError(
                        "历史回款启动缺少完整提交收据，不能确认本次载荷".into(),
                    ));
                }
                current.ensure_draft_editor(input.actor.id())?;
                ensure_expected_version(current.base.version, input.receipt.base.version)?;
                let context = receipt_command_context(&command, &input.actor)?;
                let receipt = persist_customer_receipt_start_apply(&db, input, executor).await?;
                persist_receipt_command_success(
                    &MongoReceiptCommandWrites { db: &db },
                    &command,
                    &context,
                    &receipt,
                    executor,
                )
                .await?;
                Ok(receipt)
            })
        })
        .await
}

/// 资格和原登记人事实必须在当前执行器内重新取得，不能沿用缓存授权。
async fn authorized_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<CustomerReceipt> {
    ensure_receipt_edit_authorized(db, rbac, actor, id, executor).await?;
    let receipt = db
        .customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
    ensure_receipt_owner(&receipt, actor.id())?;
    Ok(receipt)
}

/// 使用相同执行器核验完整命令事实，并拒绝跨原单或损坏收据。
async fn committed_submit(
    db: &Database,
    command: &CommandReceipt,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let committed =
        FinanceCommandReceiptService::new(db.clone()).committed_resource_id(command, executor).await?;
    match committed {
        Some(committed_id) => {
            ensure_committed_resource(&committed_id, Some(id))?;
            Ok(true)
        },
        None => Ok(false),
    }
}

/// 同一事务的授权资格必须先于财务回执及原结果读取。
#[async_trait]
trait ReceiptSubmitReplayPort: ReceiptReplayPort {
    /// 在当前执行器重验账号、提交资格、资金来源和原登记人。
    async fn authorize(&self, id: &str, executor: &mut dyn Executor) -> Result<()>;
}

/// 首错顺序沿用账号、提交资格、完整资金来源、原登记人、回执与当前视图。
async fn replay_authorized_submit<P: ReceiptSubmitReplayPort>(
    port: &P,
    id: &str,
    command: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<P::Output>> {
    port.authorize(id, executor).await?;
    replay_receipt_command(port, command, Some(id), executor).await
}

struct MongoReceiptSubmitReplay<'a> {
    db: &'a Database,
    rbac: &'a SharedRbacService,
    actor: &'a AuditActor,
}

#[async_trait]
impl ReceiptReplayPort for MongoReceiptSubmitReplay<'_> {
    type Output = CustomerReceiptView;

    async fn committed_resource_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(command, executor)
            .await?)
    }

    async fn current_result(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerReceiptView> {
        receipt_view_with_actions(self.db, self.rbac, self.actor, id, executor).await
    }
}

#[async_trait]
impl ReceiptSubmitReplayPort for MongoReceiptSubmitReplay<'_> {
    async fn authorize(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        authorized_receipt(self.db, self.rbac, self.actor, id, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_core::ids::ReceivableEntryId;
    use erp_finance::Error as FinanceError;
    use erp_finance::dto::receivable::ReceiptAllocationLineRequest;
    use erp_finance::entity::command_receipt::FinanceCommandReceipt;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    /// 同一操作重试可精确恢复；改变版本、来源、核销金额或原单均不能回放。
    #[test]
    fn submit_receipt_requires_the_complete_original_payload() {
        let request = SubmitCustomerReceiptRequest {
            expected_version: 2,
            idempotency_key: "retry".into(),
            allocations: vec![ReceiptAllocationLineRequest {
                receivable_entry_id: ReceivableEntryId::new("entry"),
                allocated_amount: "50.00".parse().unwrap(),
            }],
        };
        let command = submit_receipt("receipt", &request, "creator").unwrap();
        let fact = FinanceCommandReceipt::resource(&command, "receipt".into(), "audit-event".into()).unwrap();
        let replay = submit_receipt("receipt", &request, "creator").unwrap();
        assert_eq!(fact.resource_id(&replay).unwrap(), "receipt");
        assert!(ensure_committed_resource(&fact.resource_id(&replay).unwrap(), Some("other")).is_err());
        let mut changed_version = request.clone();
        changed_version.expected_version += 1;
        let mut changed_amount = request.clone();
        changed_amount.allocations[0].allocated_amount = "40.00".parse().unwrap();
        let mut changed_source = request.clone();
        changed_source.allocations[0].receivable_entry_id = ReceivableEntryId::new("other");
        for changed in [changed_version, changed_amount, changed_source] {
            let command = submit_receipt("receipt", &changed, "creator").unwrap();
            assert!(matches!(fact.resource_id(&command), Err(FinanceError::ConflictError(_))));
        }
        let other_order = submit_receipt("other", &request, "creator").unwrap();
        assert!(matches!(fact.resource_id(&other_order), Err(FinanceError::ConflictError(_))));
        let other_actor = submit_receipt("receipt", &request, "other").unwrap();
        assert!(matches!(fact.resource_id(&other_actor), Err(FinanceError::Internal(_))));
    }

    fn actor() -> AuditActor {
        AuditActor::new("creator".into(), "fukuan".into(), AccountKind::Admin)
    }

    fn request() -> SubmitCustomerReceiptRequest {
        SubmitCustomerReceiptRequest {
            expected_version: 2,
            idempotency_key: "retry".into(),
            allocations: vec![ReceiptAllocationLineRequest {
                receivable_entry_id: ReceivableEntryId::new("entry"),
                allocated_amount: "50.00".parse().unwrap(),
            }],
        }
    }

    struct SubmitReplay {
        identity: usize,
        denied: bool,
        receipt: Option<FinanceCommandReceipt>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl SubmitReplay {
        fn visit(&self, executor: &mut dyn Executor, call: &'static str) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            self.calls.lock().unwrap().push(call);
        }
    }

    #[async_trait]
    impl ReceiptReplayPort for SubmitReplay {
        type Output = u32;

        async fn committed_resource_id(
            &self,
            command: &CommandReceipt,
            executor: &mut dyn Executor,
        ) -> Result<Option<String>> {
            self.visit(executor, "receipt");
            self.receipt.as_ref().map(|receipt| receipt.resource_id(command).map_err(Error::from)).transpose()
        }

        async fn current_result(&self, id: &str, executor: &mut dyn Executor) -> Result<u32> {
            self.visit(executor, "view");
            assert_eq!(id, "receipt");
            Ok(17)
        }
    }

    #[async_trait]
    impl ReceiptSubmitReplayPort for SubmitReplay {
        async fn authorize(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
            self.visit(executor, "authorize");
            assert_eq!(id, "receipt");
            if self.denied {
                return Err(Error::Forbidden("原资格拒绝".into()));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn current_authorization_precedes_replay_and_payload_conflict_on_same_executor() {
        for (denied, changed, expected_steps) in [
            (false, false, vec!["authorize", "receipt", "view"]),
            (true, true, vec!["authorize"]),
            (false, true, vec!["authorize", "receipt"]),
        ] {
            let mut executor = NoTransaction;
            let original = submit_receipt("receipt", &request(), "creator").unwrap();
            let probe = SubmitReplay {
                identity: &mut executor as *mut NoTransaction as usize,
                denied,
                receipt: Some(
                    FinanceCommandReceipt::resource(&original, "receipt".into(), "event".into()).unwrap(),
                ),
                calls: Mutex::new(vec![]),
            };
            let mut changed_request = request();
            if changed {
                changed_request.expected_version += 1;
            }
            let command = submit_receipt("receipt", &changed_request, "creator").unwrap();
            let result = replay_authorized_submit(&probe, "receipt", &command, &mut executor).await;
            if denied {
                assert!(matches!(result, Err(Error::Forbidden(message)) if message == "原资格拒绝"));
            } else if changed {
                assert!(matches!(result, Err(Error::ConflictError(_))));
            } else {
                assert_eq!(result.unwrap(), Some(17));
            }
            assert_eq!(*probe.calls.lock().unwrap(), expected_steps);
        }
    }

    struct RecoveryProbe {
        result: Mutex<Option<Result<Option<u32>>>>,
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl ReceiptSubmitRecoveryPort for RecoveryProbe {
        type Output = u32;

        async fn probe(&self, id: &str, command: &CommandReceipt, actor: &AuditActor) -> Result<Option<u32>> {
            assert_eq!(id, "receipt");
            assert_eq!(command, &submit_receipt("receipt", &request(), actor.id()).unwrap());
            *self.calls.lock().unwrap() += 1;
            self.result.lock().unwrap().take().unwrap_or(Ok(None))
        }
    }

    fn unknown_error() -> Error {
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
            "original submit unknown",
        )))
    }

    #[tokio::test]
    async fn unknown_submit_keeps_original_error_for_authorization_receipt_or_view_failure() {
        for probe_error in [
            Error::Forbidden("原资格拒绝".into()),
            Error::ConflictError("同键异载荷".into()),
            Error::Internal("回执损坏".into()),
            Error::NotFound("原单视图不可用".into()),
        ] {
            let probe = RecoveryProbe { result: Mutex::new(Some(Err(probe_error))), calls: Mutex::new(0) };
            let command = submit_receipt("receipt", &request(), "creator").unwrap();
            let error = recover_authorized_submit(&probe, "receipt", &command, &actor(), unknown_error())
                .await
                .unwrap_err();
            match error {
                Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) => {
                    assert_eq!(source.get_custom::<&str>(), Some(&"original submit unknown"));
                },
                other => panic!("原未知错误被替换: {other:?}"),
            }
            assert_eq!(*probe.calls.lock().unwrap(), 1);
        }
    }

    #[tokio::test]
    async fn unknown_submit_missing_receipt_stays_bounded_and_exact_success_returns_original_view() {
        for (found, expected_calls) in [(false, 8), (true, 1)] {
            let probe = RecoveryProbe {
                result: Mutex::new(Some(Ok(if found { Some(17) } else { None }))),
                calls: Mutex::new(0),
            };
            let command = submit_receipt("receipt", &request(), "creator").unwrap();
            let result =
                recover_authorized_submit(&probe, "receipt", &command, &actor(), unknown_error()).await;
            if found {
                assert_eq!(result.unwrap(), 17);
            } else {
                assert!(matches!(result, Err(Error::OutcomeUnknown(_))));
            }
            assert_eq!(*probe.calls.lock().unwrap(), expected_calls);
        }
    }
}
