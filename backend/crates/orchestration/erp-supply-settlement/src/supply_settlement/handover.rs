//! 结算对账负责人交接与差异处理人改派。

use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::execution::persist_log;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::AccountKind;
use erp_fulfillment_execution::supply_execution::receipt::persist_supply_receipt;
use erp_identity::service::organization_access::ensure_org_enabled;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_permissions::supplier_settlement;
use erp_supply::command_receipt::SupplyCommandResult;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::dto::supplier_settlement::{
    HandoverCandidateView, HandoverSettlementRequest, HandoverSettlementView,
    ReassignSettlementDifferenceHandlerRequest, ReassignSettlementDifferenceHandlerView,
};
use erp_supply::entity::supplier_settlement::SupplierSettlementStatement;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::SettlementAccess;
use erp_workflow::ports::WorkflowAuthorizationPort;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use sha2::{Digest, Sha256};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::{Error, Result};

/// 对账交接与差异处理改派保持独立的命令身份及恢复结果。
#[derive(Clone, Copy)]
enum ResponsibilityChange {
    Handover,
    ReassignDifferenceHandler,
}

impl ResponsibilityChange {
    /// 返回当前操作登记的审计与回执动作。
    fn action(self) -> &'static str {
        match self {
            Self::Handover => "supplier_settlement.handover",
            Self::ReassignDifferenceHandler => "supplier_settlement.reassign_difference_handler",
        }
    }

    /// 与动作同时确定成功回执种类，禁止由任意字符串推断结果。
    fn result(self) -> SupplyCommandResult {
        match self {
            Self::Handover => SupplyCommandResult::SettlementHandover,
            Self::ReassignDifferenceHandler => SupplyCommandResult::DifferenceHandlerReassigned,
        }
    }

    /// 损坏或跨操作回执保持原入口的内部错误。
    fn verify_result(self, result: &SupplyCommandResult) -> Result<()> {
        if result == &self.result() {
            return Ok(());
        }
        let message = match self {
            Self::Handover => "结算交接回执类型非法",
            Self::ReassignDifferenceHandler => "差异处理人改派回执类型非法",
        };
        Err(Error::Internal(message.to_string()))
    }
}

/// 同一次责任变更沿用的身份、原载荷和幂等键；不包含业务请求字段。
struct ResponsibilityCommand {
    actor: AuditActor,
    statement_id: String,
    audit_id: String,
    fingerprint: String,
    key: String,
    change: ResponsibilityChange,
}

impl ResponsibilityCommand {
    /// 生成与既有入口相同的稳定命令编号，幂等键仅在持久化时保存摘要。
    fn new(
        actor: &AuditActor,
        statement_id: &str,
        key: &str,
        change: ResponsibilityChange,
        fingerprint: String,
    ) -> Self {
        let prefix = match change {
            ResponsibilityChange::Handover => "settlement-handover-",
            ResponsibilityChange::ReassignDifferenceHandler => "settlement-diff-handler-",
        };
        Self {
            actor: actor.clone(),
            statement_id: statement_id.to_string(),
            audit_id: format!("{prefix}{}", digest(&[actor.id(), statement_id, key.trim()])),
            fingerprint,
            key: key.to_string(),
            change,
        }
    }

    /// 从本次命令身份构造与业务更新同事务保存的成功事件。
    fn audit(&self, number: Option<String>) -> Result<AuditLog> {
        Ok(self
            .actor
            .clone()
            .resource_log_with_id(
                self.audit_id.clone(),
                self.change.action(),
                "supplier_settlement_statement",
                self.statement_id.clone(),
                Some("供应商结算责任已更新".to_string()),
            )?
            .with_command_id(Some(self.audit_id.clone()))?
            .with_resource_number(number)?)
    }
}

/// 两个责任命令共用的数据库、当前范围授权与人员资格依赖。
struct SettlementDependencies {
    db: Database,
    access: SettlementAccess,
    authorization: Arc<AuthorizationService>,
}

impl SettlementDependencies {
    fn from_process<A: WorkflowAuthorizationPort>(process: &SupplierSettlementProcess<A>) -> Self {
        Self {
            db: process.db.clone(),
            access: process.data_scope.clone(),
            authorization: process.authorization.clone(),
        }
    }

    /// 业务更新之后依次保存独立命令回执与展示审计，沿用同一执行器。
    async fn write_audit(
        &self,
        command: &ResponsibilityCommand,
        number: Option<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let audit = command.audit(number)?;
        persist_supply_receipt(
            &self.db,
            &audit,
            &command.fingerprint,
            &command.key,
            &command.statement_id,
            command.change.result(),
            executor,
        )
        .await
        .map_err(Error::from)?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }
}

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 显式交接对账负责人；开放复核任务不改派。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 目标人员、可选组织、原因、版本与幂等键
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回交接后的对账负责人、业务组织与版本。
    ///
    /// # 错误
    /// 目标不合格、版本冲突、范围不足或责任未变化时拒绝。
    pub async fn handover_statement(
        &self,
        id: &str,
        req: HandoverSettlementRequest,
        actor: &AuditActor,
    ) -> Result<HandoverSettlementView> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.handover_statement_apply(&id, req, &actor, executor).await })
            })
            .await
    }

    /// 在入口事务内完成授权、准备、重放与持久化。
    async fn handover_statement_apply(
        &self,
        id: &str,
        req: HandoverSettlementRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<HandoverSettlementView> {
        req.validate()?;
        let command = ResponsibilityCommand::new(
            actor,
            id,
            &req.idempotency_key,
            ResponsibilityChange::Handover,
            handover_fingerprint(actor.id(), id, &req),
        );
        if let Some(statement) = self.replay_responsibility_change(&command, executor).await? {
            return Ok(handover_view(statement));
        }
        ensure_target_qualified(&self.authorization, req.target_user_id.trim(), executor).await?;
        ensure_org_enabled(&self.db, req.target_org_unit_id.as_deref(), executor).await?;
        let dependencies = SettlementDependencies::from_process(self);
        apply_handover(&dependencies, &command, &req, executor).await
    }

    /// 独立改派差异处理人。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 目标人员、原因、版本与幂等键
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回改派后的差异处理人与版本。
    ///
    /// # 错误
    /// 目标不合格、版本冲突或目标已是当前处理人时拒绝。
    pub async fn reassign_difference_handler(
        &self,
        id: &str,
        req: ReassignSettlementDifferenceHandlerRequest,
        actor: &AuditActor,
    ) -> Result<ReassignSettlementDifferenceHandlerView> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.reassign_difference_handler_apply(&id, req, &actor, executor).await
                })
            })
            .await
    }

    /// 在入口事务内完成授权、准备、重放与持久化。
    async fn reassign_difference_handler_apply(
        &self,
        id: &str,
        req: ReassignSettlementDifferenceHandlerRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<ReassignSettlementDifferenceHandlerView> {
        req.validate()?;
        let command = ResponsibilityCommand::new(
            actor,
            id,
            &req.idempotency_key,
            ResponsibilityChange::ReassignDifferenceHandler,
            handler_fingerprint(actor.id(), id, &req),
        );
        if let Some(statement) = self.replay_responsibility_change(&command, executor).await? {
            return Ok(handler_view(statement));
        }
        ensure_target_qualified(&self.authorization, req.target_user_id.trim(), executor).await?;
        let dependencies = SettlementDependencies::from_process(self);
        apply_handler(&dependencies, &command, &req, executor).await
    }

    /// 查询可交接的合格有效人员。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回具备结算维护资格的有效人员。
    ///
    /// # 错误
    /// 结算单不在更新范围内时拒绝。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<HandoverCandidateView>> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.handover_candidates_apply(&id, &actor, executor).await })
            })
            .await
    }

    /// 在同一读取快照内检查结算范围并返回合格交接人。
    async fn handover_candidates_apply(
        &self,
        id: &str,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<Vec<HandoverCandidateView>> {
        let current = self
            .domain()
            .access()
            .require_statement(actor, &supplier_settlement::STATEMENT_REFRESH, id, executor)
            .await?;
        list_candidates(&self.authorization, &self.rbac, &current.prepared_by, executor).await
    }

    async fn replay_responsibility_change(
        &self,
        command: &ResponsibilityCommand,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        let Some(stored) =
            self.db.supply_command_receipts().find_command(&command.audit_id, executor).await?
        else {
            return Ok(None);
        };
        stored.verify_identity(
            &command.audit_id,
            command.actor.id(),
            command.change.action(),
            &command.statement_id,
            &stable_digest(command.key.trim()),
        )?;
        let conflict_message = match command.change {
            ResponsibilityChange::Handover => "同一幂等键已用于不同的结算交接",
            ResponsibilityChange::ReassignDifferenceHandler => "同一幂等键已用于不同的差异处理人改派",
        };
        stored.verify(&command.fingerprint, Some(&command.statement_id), conflict_message)?;
        command.change.verify_result(&stored.result)?;
        self.data_scope
            .require_statement(
                &command.actor,
                &supplier_settlement::STATEMENT_REFRESH,
                &command.statement_id,
                executor,
            )
            .await?;
        Ok(Some(self.domain().load_statement(&command.statement_id, executor).await?))
    }
}

/// 交接在事务内重验范围、版本和目标资格，再保存新责任及命令结果。
async fn apply_handover(
    dependencies: &SettlementDependencies,
    command: &ResponsibilityCommand,
    req: &HandoverSettlementRequest,
    executor: &mut dyn Executor,
) -> Result<HandoverSettlementView> {
    let mut statement = dependencies
        .access
        .require_statement(
            &command.actor,
            &supplier_settlement::STATEMENT_REFRESH,
            &command.statement_id,
            executor,
        )
        .await?;
    if statement.base.version != req.expected_version {
        return Err(Error::ConflictError("结算单责任或版本已变化，请刷新后重试".into()));
    }
    ensure_target_qualified(&dependencies.authorization, req.target_user_id.trim(), executor).await?;
    ensure_org_enabled(&dependencies.db, req.target_org_unit_id.as_deref(), executor).await?;
    let next_org = req.target_org_unit_id.clone().filter(|value| !value.trim().is_empty());
    statement.handover(req.target_user_id.clone(), next_org)?;
    dependencies.access.require_owner_change(&command.actor, &statement, executor).await?;
    dependencies.db.supplier_settlement_statements().update(&mut statement, executor).await?;
    dependencies.write_audit(command, Some(statement.statement_no.clone()), executor).await?;
    Ok(handover_view(statement))
}

/// 差异处理人独立改派，事务授权与保存顺序和对账责任交接一致。
async fn apply_handler(
    dependencies: &SettlementDependencies,
    command: &ResponsibilityCommand,
    req: &ReassignSettlementDifferenceHandlerRequest,
    executor: &mut dyn Executor,
) -> Result<ReassignSettlementDifferenceHandlerView> {
    let mut statement = dependencies
        .access
        .require_statement(
            &command.actor,
            &supplier_settlement::STATEMENT_REFRESH,
            &command.statement_id,
            executor,
        )
        .await?;
    if statement.base.version != req.expected_version {
        return Err(Error::ConflictError("结算单责任或版本已变化，请刷新后重试".into()));
    }
    ensure_target_qualified(&dependencies.authorization, req.target_user_id.trim(), executor).await?;
    statement.reassign_difference_handler(req.target_user_id.clone())?;
    dependencies.db.supplier_settlement_statements().update(&mut statement, executor).await?;
    dependencies.write_audit(command, Some(statement.statement_no.clone()), executor).await?;
    Ok(handler_view(statement))
}

fn handover_view(statement: SupplierSettlementStatement) -> HandoverSettlementView {
    HandoverSettlementView {
        statement_id: statement.base.id,
        prepared_by: statement.prepared_by,
        business_org_unit_id: statement.business_org_unit_id,
        version: statement.base.version,
    }
}

fn handler_view(statement: SupplierSettlementStatement) -> ReassignSettlementDifferenceHandlerView {
    let difference_handler_user_id = statement.difference_handler().to_string();
    ReassignSettlementDifferenceHandlerView {
        statement_id: statement.base.id,
        difference_handler_user_id,
        version: statement.base.version,
    }
}

async fn ensure_target_qualified(
    authorization: &AuthorizationService,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let actor = AuditActor::new(target.into(), target.into(), AccountKind::Admin);
    let permission = supplier_settlement::STATEMENT_REFRESH;
    if !authorization.has_permission(&actor, &permission, executor).await? {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备结算维护资格".into()));
    }
    Ok(())
}

async fn list_candidates(
    authorization: &AuthorizationService,
    rbac: &SharedRbacService,
    current: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<HandoverCandidateView>> {
    let accounts = rbac.directory().accounts_by_kind(AccountKind::Admin, executor).await?;
    let mut candidates = Vec::new();
    for account in accounts {
        if account.id == current {
            continue;
        }
        let required = [supplier_settlement::STATEMENT_REFRESH];
        let actor = AuditActor::new(account.id, account.login_account, account.kind);
        let qualified = authorization.permissions(&actor, &required, executor).await?;
        let Some(qualified) = qualified.filter(|value| value.same_role(&required)) else { continue };
        let account = qualified.account();
        candidates.push(HandoverCandidateView {
            user_id: account.id.clone(),
            display_name: account.display_name.clone(),
            account: account.login_account.clone(),
        });
    }
    candidates.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    Ok(candidates)
}

fn handover_fingerprint(actor_id: &str, id: &str, req: &HandoverSettlementRequest) -> String {
    digest(&[
        actor_id,
        id,
        req.target_user_id.trim(),
        req.target_org_unit_id.as_deref().unwrap_or(""),
        req.reason.trim(),
        &req.expected_version.to_string(),
        req.idempotency_key.trim(),
    ])
}

fn handler_fingerprint(actor_id: &str, id: &str, req: &ReassignSettlementDifferenceHandlerRequest) -> String {
    digest(&[
        actor_id,
        id,
        req.target_user_id.trim(),
        req.reason.trim(),
        &req.expected_version.to_string(),
        req.idempotency_key.trim(),
    ])
}

fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use erp_core::Error as CoreError;

    use super::*;

    /// 执行生产审计构造，两个动作分别生成自己的回执种类和事件身份。
    #[test]
    fn responsibility_commands_keep_distinct_actions_results_and_event_identity() {
        let actor = AuditActor::new("actor-1".into(), "operator".into(), AccountKind::Admin)
            .with_request_id(Some("request-1".into()))
            .unwrap();
        let cases = [
            (
                ResponsibilityChange::Handover,
                "supplier_settlement.handover",
                SupplyCommandResult::SettlementHandover,
            ),
            (
                ResponsibilityChange::ReassignDifferenceHandler,
                "supplier_settlement.reassign_difference_handler",
                SupplyCommandResult::DifferenceHandlerReassigned,
            ),
        ];
        let mut command_ids = Vec::new();
        for (change, action, expected_result) in cases {
            let command =
                ResponsibilityCommand::new(&actor, "statement-1", " private-key ", change, "a".repeat(64));
            let audit = command.audit(Some("JS202610040001".into())).unwrap();
            assert_eq!(audit.action, action);
            assert_eq!(audit.resource_type, "supplier_settlement_statement");
            assert_eq!(audit.resource_id.as_deref(), Some("statement-1"));
            assert_eq!(audit.actor_id, "actor-1");
            assert_eq!(change.result(), expected_result);
            let event = audit.structured_event.unwrap();
            assert_eq!(event.command_id.as_deref(), Some(command.audit_id.as_str()));
            assert_eq!(event.resource_number_snapshot.as_deref(), Some("JS202610040001"));
            assert_eq!(event.request_id.as_deref(), Some("request-1"));
            command_ids.push(command.audit_id);
        }
        assert_ne!(command_ids[0], command_ids[1]);
    }

    /// 重试键去首尾空白，操作人、结算单与操作种类仍属于命令身份。
    #[test]
    fn responsibility_command_identity_normalizes_key_and_keeps_actor_and_scope() {
        let actor = AuditActor::new("actor-1".into(), "operator".into(), AccountKind::Admin);
        let other_actor = AuditActor::new("actor-2".into(), "operator".into(), AccountKind::Admin);
        let command = |actor: &AuditActor, id, key| {
            ResponsibilityCommand::new(actor, id, key, ResponsibilityChange::Handover, "a".repeat(64))
        };
        let original = command(&actor, "statement-1", " private-key ");
        assert_eq!(original.audit_id, command(&actor, "statement-1", "private-key").audit_id);
        assert_ne!(original.audit_id, command(&other_actor, "statement-1", "private-key").audit_id);
        assert_ne!(original.audit_id, command(&actor, "statement-2", "private-key").audit_id);
        assert_ne!(original.audit_id, command(&actor, "statement-1", "other-key").audit_id);
    }

    /// 两种恢复入口只接受对应回执，跨动作结果保持明确内部错误。
    #[test]
    fn responsibility_result_replay_accepts_own_result_and_rejects_other_actions() {
        for (change, wrong_result, message) in [
            (
                ResponsibilityChange::Handover,
                SupplyCommandResult::DifferenceHandlerReassigned,
                "结算交接回执类型非法",
            ),
            (
                ResponsibilityChange::ReassignDifferenceHandler,
                SupplyCommandResult::SettlementHandover,
                "差异处理人改派回执类型非法",
            ),
        ] {
            change.verify_result(&change.result()).unwrap();
            assert!(matches!(
                change.verify_result(&wrong_result),
                Err(Error::Internal(actual)) if actual == message
            ));
            assert!(matches!(
                change.verify_result(&SupplyCommandResult::CapabilitiesUpdated),
                Err(Error::Internal(actual)) if actual == message
            ));
        }
    }

    /// 无效审计身份继续失败关闭，不能生成供回执持久化的成功事件。
    #[test]
    fn responsibility_audit_rejects_invalid_actor_and_resource_number() {
        let invalid_actor = AuditActor::new(String::new(), "operator".into(), AccountKind::Admin);
        let command = ResponsibilityCommand::new(
            &invalid_actor,
            "statement-1",
            "private-key",
            ResponsibilityChange::Handover,
            "a".repeat(64),
        );
        assert!(matches!(
            command.audit(None),
            Err(Error::Logic(CoreError::LogicError(message))) if message == "操作人ID不能为空"
        ));

        let valid_actor = AuditActor::new("actor-1".into(), "operator".into(), AccountKind::Admin);
        let command = ResponsibilityCommand::new(
            &valid_actor,
            "statement-1",
            "private-key",
            ResponsibilityChange::Handover,
            "a".repeat(64),
        );
        assert!(matches!(
            command.audit(Some("forged\nnumber".into())),
            Err(Error::ValidationError(message)) if message == "业务编号包含非法字符"
        ));
    }
}
