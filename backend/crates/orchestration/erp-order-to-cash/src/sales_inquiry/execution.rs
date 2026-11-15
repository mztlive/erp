//! 一个命令一个事务；重放不产生新的任务、审计或业务事实。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_permissions::{Permission as AccessPermission, sales_inquiry as sales_inquiry_permissions};
use erp_sales::entity::sales_inquiry::{InquiryCommandReceipt, InquiryCommandResult, SalesInquiry};
use erp_sales::service::sales_inquiry::SalesInquiryService;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::SalesInquiryProcess;
use super::dto::*;
use super::fee_allocation::AllocateInquiryFeesCommand;
use super::internal_quote::{SaveInquiryFeeBasisCommand, SaveInquiryQuoteCommand};
use super::quote_import::ImportInquiryQuotesCommand;
use crate::{Error, Result};

mod apply;
mod create;

#[derive(Clone, Serialize)]
#[serde(tag = "action", content = "request", rename_all = "snake_case")]
pub(super) enum InquiryCommand {
    Create(CreateInquiryCommand),
    Save(SaveInquiryCommand),
    Submit(SubmitInquiryCommand),
    Supplement(InquiryTaskReasonCommand),
    SaveFeedback(SaveInquiryFeedbackCommand),
    Publish(PublishInquiryCommand),
    Renew(InquiryTaskReasonCommand),
    Accept(AcceptInquiryCommand),
    Reinquire(ReinquireInquiryCommand),
    Close(InquiryReasonCommand),
    Reassign(ReassignInquiryCommand),
    AllocateFees(AllocateInquiryFeesCommand),
    InternalQuote(Box<SaveInquiryQuoteCommand>),
    ImportQuotes(Box<ImportInquiryQuotesCommand>),
    FeeBasis(SaveInquiryFeeBasisCommand),
}

impl InquiryCommand {
    fn action(&self) -> &'static str {
        match self {
            Self::Create(_) => "create",
            Self::Save(_) => "save",
            Self::Submit(_) => "submit",
            Self::Supplement(_) => "supplement",
            Self::SaveFeedback(_) => "save_feedback",
            Self::Publish(_) => "publish",
            Self::Renew(_) => "renew",
            Self::Accept(_) => "accept",
            Self::Reinquire(_) => "reinquire",
            Self::Close(_) => "close",
            Self::Reassign(_) => "reassign",
            Self::AllocateFees(_) => "allocate_fees",
            Self::InternalQuote(_) => "internal_quote",
            Self::ImportQuotes(_) => "import_quotes",
            Self::FeeBasis(_) => "fee_basis",
        }
    }
    fn key(&self) -> &str {
        match self {
            Self::Create(r) => &r.command_key,
            Self::Save(r) => &r.command_key,
            Self::Submit(r) => &r.command_key,
            Self::Publish(r) => &r.command_key,
            Self::Renew(r) => &r.command_key,
            Self::Supplement(r) => &r.command_key,
            Self::SaveFeedback(r) => &r.command_key,
            Self::Accept(r) => &r.command_key,
            Self::Reinquire(r) => &r.command_key,
            Self::Close(r) => &r.command_key,
            Self::Reassign(r) => &r.command_key,
            Self::AllocateFees(r) => &r.command_key,
            Self::InternalQuote(r) => &r.command_key,
            Self::ImportQuotes(r) => &r.command_key,
            Self::FeeBasis(r) => &r.command_key,
        }
    }
    fn task(&self) -> Option<(&str, u64)> {
        match self {
            Self::Publish(r) => Some((&r.task_id, r.task_version)),
            Self::Renew(r) => Some((&r.task_id, r.task_version)),
            Self::Supplement(r) => Some((&r.task_id, r.task_version)),
            Self::SaveFeedback(r) => Some((&r.task_id, r.task_version)),
            Self::Reassign(r) => Some((&r.task_id, r.task_version)),
            Self::InternalQuote(r) => Some((&r.task_id, r.task_version)),
            Self::ImportQuotes(r) => Some((&r.task_id, r.task_version)),
            Self::FeeBasis(r) => Some((&r.task_id, r.task_version)),
            _ => None,
        }
    }
    /// 由销售命令本身选择公共入口，采购命令不能借用销售资格。
    fn sales_permission(&self) -> Result<AccessPermission> {
        Ok(match self {
            Self::Create(_) => sales_inquiry_permissions::COMMANDS_SALES_CREATE,
            Self::Save(_) | Self::AllocateFees(_) => sales_inquiry_permissions::COMMANDS_SALES_SAVE,
            Self::Submit(_) => sales_inquiry_permissions::COMMANDS_SALES_SUBMIT,
            Self::Accept(_) => sales_inquiry_permissions::COMMANDS_SALES_ACCEPT,
            Self::Reinquire(_) => sales_inquiry_permissions::COMMANDS_SALES_REINQUIRE,
            Self::Close(_) => sales_inquiry_permissions::COMMANDS_SALES_CLOSE,
            _ => return Err(Error::ValidationError("采购命令不能使用销售授权入口".into())),
        })
    }
    /// 由精确采购命令选择固定任务入口，销售命令不能借用采购资格。
    fn procurement_permission(&self) -> Result<AccessPermission> {
        Ok(match self {
            Self::Publish(_) | Self::Renew(_) => sales_inquiry_permissions::COMMANDS_PROCUREMENT_PUBLISH,
            Self::Supplement(_) => sales_inquiry_permissions::COMMANDS_PROCUREMENT_SUPPLEMENT,
            Self::SaveFeedback(_) => sales_inquiry_permissions::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
            Self::Reassign(_) => sales_inquiry_permissions::COMMANDS_PROCUREMENT_REASSIGN,
            Self::InternalQuote(_) | Self::ImportQuotes(_) | Self::FeeBasis(_) => {
                sales_inquiry_permissions::INTERNAL_FEE_BASES
            },
            _ => return Err(Error::ValidationError("销售命令不能使用采购授权入口".into())),
        })
    }
}

impl SalesInquiryProcess {
    pub(super) async fn execute(
        &self,
        actor: &AuditActor,
        id: &str,
        command: InquiryCommand,
    ) -> Result<InquiryMutationView> {
        let receipt = CommandReceipt::from_payload(
            "sales-inquiry-command:",
            actor.id(),
            command.action(),
            "sales_inquiry",
            command.key(),
            &(id, &command),
        )?;
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction({
                let actor = actor.clone();
                let id = id.clone();
                let command = command.clone();
                let receipt = receipt.clone();
                move |executor| {
                    Box::pin(
                        async move { this.execute_command(&actor, &id, command, receipt, executor).await },
                    )
                }
            })
            .await;
        match result {
            Err(error) if error.command_may_have_committed() => {
                self.recovered_result(&actor, &id, &command, &receipt, error).await
            },
            other => other,
        }
    }

    /// 未知提交只读取原收据并在独立恢复快照重新授权，不重放写命令。
    async fn recovered_result(
        &self,
        actor: &AuditActor,
        id: &str,
        command: &InquiryCommand,
        receipt: &CommandReceipt,
        original_error: Error,
    ) -> Result<InquiryMutationView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let command = command.clone();
        let receipt = receipt.clone();
        let recovered = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Ok(Some(prior)) = this.receipt(receipt.id(), executor).await else {
                        return Ok(None);
                    };
                    replay_receipt(&this, &actor, &id, &command, &receipt, &prior, executor).await.map(Some)
                })
            })
            .await?;
        recovered.ok_or(original_error)
    }

    async fn execute_command(
        &self,
        actor: &AuditActor,
        id: &str,
        command: InquiryCommand,
        receipt: CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<InquiryMutationView> {
        if let Some(result) = replay(self, actor, id, &command, &receipt, executor).await? {
            return Ok(result);
        }
        let (mut inquiry, tasks) = self.apply_command(actor, id, &command, executor).await?;
        if !matches!(command, InquiryCommand::Create(_)) {
            SalesInquiryService::new(self.db.clone()).save(&mut inquiry, executor).await?;
        }
        let result = mutation_view(&inquiry, tasks);
        self.persist_command(actor, &command, &receipt, &result, executor).await?;
        Ok(result)
    }

    async fn persist_command(
        &self,
        actor: &AuditActor,
        command: &InquiryCommand,
        receipt: &CommandReceipt,
        result: &InquiryMutationView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            &format!("sales_inquiry.{}", command.action()),
            "sales_inquiry",
            result.inquiry_id.clone(),
        )?;
        persist_log(&self.db, &audit, executor).await?;
        let record = InquiryCommandReceipt::new(
            receipt.id().to_string(),
            actor.id(),
            command.key(),
            command.action(),
            receipt.fingerprint().digest_hex().to_string(),
            mutation_result(result),
        )?;
        SalesInquiryService::new(self.db.clone()).save_receipt(&record, executor).await?;
        Ok(())
    }
}

/// 回放只读原回执并调用现有完整授权，不具备写入业务或审计的能力。
#[async_trait]
pub(super) trait ReplaySource: Sync {
    /// 在本次快照读取完整回执。
    async fn receipt(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<InquiryCommandReceipt>>;

    /// 以原结果的真实询价重验销售动作；创建仍使用候选归属合同。
    async fn sales(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 保持采购命令回放的原动作资格，不借用销售权限。
    async fn procurement(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

#[async_trait]
impl ReplaySource for SalesInquiryProcess {
    /// 回执、授权和交付使用调用方同一个快照。
    async fn receipt(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<InquiryCommandReceipt>> {
        Ok(SalesInquiryService::new(self.db.clone()).receipt(id, executor).await?)
    }

    /// 现有销售访问器从真实对象构造创建候选或执行记录检查。
    async fn sales(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.access().require_sales(actor, permission, id, executor).await?;
        Ok(())
    }

    /// 采购回放仍只核对原命令资格，不重新办理已完成任务。
    async fn procurement(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.access().ensure_procurement_permission(actor, permission, executor).await?;
        Ok(())
    }
}

/// 核对命令身份与载荷后，授权原结果对象并直接交付冻结结果。
async fn replay(
    source: &impl ReplaySource,
    actor: &AuditActor,
    id: &str,
    command: &InquiryCommand,
    receipt: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<InquiryMutationView>> {
    let Some(prior) = source.receipt(receipt.id(), executor).await? else { return Ok(None) };
    replay_receipt(source, actor, id, command, receipt, &prior, executor).await.map(Some)
}

/// 已读回执验证与授权由普通重试和未知提交恢复共用，读取失败不被误认为授权失败。
async fn replay_receipt(
    source: &impl ReplaySource,
    actor: &AuditActor,
    id: &str,
    command: &InquiryCommand,
    receipt: &CommandReceipt,
    prior: &InquiryCommandReceipt,
    executor: &mut dyn Executor,
) -> Result<InquiryMutationView> {
    let view = mutation_from_result(prior.matches(
        actor.id(),
        command.key(),
        command.action(),
        receipt.fingerprint().digest_hex(),
    )?)?;
    if view.inquiry_id != id && !(matches!(command, InquiryCommand::Create(_)) && id.is_empty()) {
        return Err(Error::ConflictError("询价命令回执不属于当前询价".into()));
    }
    if command.task().is_some() {
        source.procurement(actor, &command.procurement_permission()?, executor).await?;
    } else {
        source.sales(actor, &command.sales_permission()?, &view.inquiry_id, executor).await?;
    }
    Ok(view)
}

fn mutation_view(inquiry: &SalesInquiry, task_ids: Vec<String>) -> InquiryMutationView {
    InquiryMutationView {
        inquiry_id: inquiry.base.id.clone(),
        version: inquiry.base.version,
        requirement_version: inquiry.requirement_version,
        feedback_round: inquiry.feedback_round,
        status: inquiry.status,
        task_ids,
    }
}

fn mutation_result(view: &InquiryMutationView) -> InquiryCommandResult {
    InquiryCommandResult::Mutation {
        inquiry_id: view.inquiry_id.clone(),
        version: view.version,
        requirement_version: view.requirement_version,
        feedback_round: view.feedback_round,
        status: view.status,
        task_ids: view.task_ids.clone(),
    }
}

fn mutation_from_result(result: &InquiryCommandResult) -> Result<InquiryMutationView> {
    match result {
        InquiryCommandResult::Mutation {
            inquiry_id,
            version,
            requirement_version,
            feedback_round,
            status,
            task_ids,
        } => Ok(InquiryMutationView {
            inquiry_id: inquiry_id.clone(),
            version: *version,
            requirement_version: *requirement_version,
            feedback_round: *feedback_round,
            status: *status,
            task_ids: task_ids.clone(),
        }),
        InquiryCommandResult::Created { .. } => Err(Error::Internal("询价命令回执缺少冻结状态".into())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_sales::Error as SalesError;
    use erp_sales::entity::sales_inquiry::{InquiryCreateData, InquiryRequirementData, InquiryStatus};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 原回执与授权窄边界替身；没有任务、审计或业务写入能力。
    struct Source {
        receipt: Option<InquiryCommandReceipt>,
        denied: bool,
        calls: Mutex<Vec<(String, String, usize)>>,
    }

    impl Source {
        /// 使用真实命令指纹和冻结结果构造待回放的原成功回执。
        fn new(command: &InquiryCommand, request_id: &str, result_id: &str) -> Self {
            let receipt = command_receipt(command, request_id);
            let receipt = InquiryCommandReceipt::new(
                receipt.id().into(),
                "sales",
                command.key(),
                command.action(),
                receipt.fingerprint().digest_hex().into(),
                mutation_result(&frozen_result(result_id)),
            )
            .unwrap();
            Self { receipt: Some(receipt), denied: false, calls: Mutex::default() }
        }

        /// 记录实际编排选择的入口、对象和执行器。
        fn record(&self, entry: &str, id: &str, executor: &mut dyn Executor) {
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            self.calls.lock().unwrap().push((
                entry.into(),
                id.into(),
                executor as *mut dyn Executor as *mut () as usize,
            ));
        }
    }

    #[async_trait]
    impl ReplaySource for Source {
        /// 从同一快照返回原成功回执。
        async fn receipt(
            &self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<InquiryCommandReceipt>> {
            self.record("receipt", id, executor);
            Ok(self.receipt.clone())
        }

        /// 返回实际销售动作授权结果，不改变冻结业务结果。
        async fn sales(
            &self,
            actor: &AuditActor,
            permission: &AccessPermission,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(actor.id(), "sales");
            self.record(&permission.to_string(), id, executor);
            if self.denied {
                return Err(Error::Forbidden("当前询价创建范围已失效".into()));
            }
            Ok(())
        }

        /// 保留采购入口并拒绝错误地转为销售动作。
        async fn procurement(
            &self,
            actor: &AuditActor,
            permission: &AccessPermission,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(actor.id(), "sales");
            self.record(&permission.to_string(), "", executor);
            Ok(())
        }
    }

    /// 构造创建请求，原调用没有已有询价身份。
    fn create_command() -> InquiryCommand {
        InquiryCommand::Create(CreateInquiryCommand {
            command_key: "same-key".into(),
            data: InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(5000),
                copied_from_inquiry_id: None,
                draft: InquiryRequirementData { lines: vec![], note: "原需求".into() },
            },
        })
    }

    /// 沿生产入口使用相同命令身份和载荷生成指纹。
    fn command_receipt(command: &InquiryCommand, id: &str) -> CommandReceipt {
        CommandReceipt::from_payload(
            "sales-inquiry-command:",
            "sales",
            command.action(),
            "sales_inquiry",
            command.key(),
            &(id, command),
        )
        .unwrap()
    }

    /// 构造原回执中的不可变业务结果。
    fn frozen_result(id: &str) -> InquiryMutationView {
        InquiryMutationView {
            inquiry_id: id.into(),
            version: 1,
            requirement_version: 0,
            feedback_round: 0,
            status: InquiryStatus::Draft,
            task_ids: vec![],
        }
    }

    /// 当前回放仍使用原调用人身份。
    fn actor() -> AuditActor {
        AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn creation_replay_authorizes_the_stored_target_and_returns_original_result_in_one_snapshot() {
        let command = create_command();
        let receipt = command_receipt(&command, "");
        let source = Source::new(&command, "", "created-inquiry");
        let mut executor = SnapshotExecutor::default();
        let expected_executor = &mut executor as *mut SnapshotExecutor as usize;
        let result = replay(&source, &actor(), "", &command, &receipt, &mut executor).await.unwrap();
        assert_eq!(result, Some(frozen_result("created-inquiry")));
        assert_eq!(
            *source.calls.lock().unwrap(),
            [
                ("receipt".into(), receipt.id().into(), expected_executor),
                (
                    sales_inquiry_permissions::COMMANDS_SALES_CREATE.to_string(),
                    "created-inquiry".into(),
                    expected_executor
                ),
            ]
        );
    }

    #[tokio::test]
    async fn creation_replay_rejects_a_reused_key_with_a_different_payload_before_authorization() {
        let command = create_command();
        let source = Source::new(&command, "", "created-inquiry");
        let InquiryCommand::Create(mut changed) = command else { panic!("创建请求") };
        changed.data.draft.note = "修改需求".into();
        let changed = InquiryCommand::Create(changed);
        let receipt = command_receipt(&changed, "");
        let error = replay(&source, &actor(), "", &changed, &receipt, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "同一操作号已用于不同询价内容"));
        assert_eq!(source.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn creation_replay_denial_never_delivers_a_successful_receipt() {
        let command = create_command();
        let receipt = command_receipt(&command, "");
        let mut source = Source::new(&command, "", "created-inquiry");
        source.denied = true;
        let error = replay(&source, &actor(), "", &command, &receipt, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Forbidden(message) if message == "当前询价创建范围已失效"));
        assert_eq!(source.calls.lock().unwrap()[1].1, "created-inquiry");
    }

    #[tokio::test]
    async fn procurement_replay_keeps_the_original_task_permission_and_rejects_cross_inquiry_results() {
        let command = InquiryCommand::Publish(PublishInquiryCommand {
            command_key: "same-key".into(),
            expected_version: 2,
            task_id: "task".into(),
            task_version: 1,
        });
        let receipt = command_receipt(&command, "inquiry");
        let source = Source::new(&command, "inquiry", "inquiry");
        replay(&source, &actor(), "inquiry", &command, &receipt, &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert_eq!(
            source.calls.lock().unwrap()[1].0,
            sales_inquiry_permissions::COMMANDS_PROCUREMENT_PUBLISH.to_string()
        );
        let mismatched = Source::new(&command, "inquiry", "other-inquiry");
        let error =
            replay(&mismatched, &actor(), "inquiry", &command, &receipt, &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "询价命令回执不属于当前询价"));
        assert_eq!(mismatched.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn explicit_creation_recovery_reads_and_authorizes_the_receipt_target_in_the_same_snapshot() {
        let command = create_command();
        let receipt = command_receipt(&command, "");
        let source = Source::new(&command, "", "created-inquiry");
        let mut executor = SnapshotExecutor::default();
        let expected_executor = &mut executor as *mut SnapshotExecutor as usize;
        let result =
            super::super::recovery::recover(&source, &actor(), "", "create", receipt.id(), &mut executor)
                .await
                .unwrap();
        assert_eq!(result, Some(frozen_result("created-inquiry")));
        assert_eq!(
            *source.calls.lock().unwrap(),
            [
                ("receipt".into(), receipt.id().into(), expected_executor),
                (
                    sales_inquiry_permissions::RECOVERY_SALES_RECOVER.to_string(),
                    "created-inquiry".into(),
                    expected_executor
                ),
            ]
        );
    }

    #[test]
    fn command_identity_replays_exact_payload_and_rejects_cross_inquiry_key_reuse() {
        let command = InquiryCommand::Publish(PublishInquiryCommand {
            command_key: "same-key".into(),
            expected_version: 2,
            task_id: "task".into(),
            task_version: 1,
        });
        let make = |id: &str| {
            CommandReceipt::from_payload(
                "sales-inquiry-command:",
                "buyer",
                command.action(),
                "sales_inquiry",
                command.key(),
                &(id, &command),
            )
            .unwrap()
        };
        let receipt = make("a");
        let result = InquiryMutationView {
            inquiry_id: "a".into(),
            version: 3,
            requirement_version: 1,
            feedback_round: 0,
            status: InquiryStatus::ProcurementProcessing,
            task_ids: vec!["task".into()],
        };
        let record = InquiryCommandReceipt::new(
            receipt.id().into(),
            "buyer",
            "same-key",
            "publish",
            receipt.fingerprint().digest_hex().into(),
            mutation_result(&result),
        )
        .unwrap();
        let replay = make("a");
        assert_eq!(receipt.id(), replay.id());
        assert_eq!(
            mutation_from_result(
                record.matches("buyer", "same-key", "publish", replay.fingerprint().digest_hex()).unwrap()
            )
            .unwrap(),
            result
        );
        let other = make("b");
        assert_eq!(receipt.id(), other.id());
        assert!(matches!(
            record.matches("buyer", "same-key", "publish", other.fingerprint().digest_hex()),
            Err(SalesError::ConflictError(_))
        ));
        let serialized = serde_json::to_string(&record).unwrap();
        assert!(!serialized.contains("same-key"));
    }
}
