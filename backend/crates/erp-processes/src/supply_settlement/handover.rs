//! 结算对账负责人交接与差异处理人改派。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{AccessControlExt, Permission, PermissionSet, SharedRbacService};
use erp_supply::dto::supplier_settlement::{
    HandoverCandidateView, HandoverSettlementRequest, HandoverSettlementView,
    ReassignSettlementDifferenceHandlerRequest, ReassignSettlementDifferenceHandlerView,
};
use erp_supply::repository::SupplierSettlementExt;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};
use validator::Validate;

use super::SupplierSettlementProcess;
use crate::{Error, Result};

impl SupplierSettlementProcess {
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
        req.validate()?;
        let audit_id = format!("settlement-handover-{}-{}-{}", actor.id(), id, req.idempotency_key.trim());
        let fingerprint = handover_fingerprint(actor.id(), id, &req)?;
        if let Some(existing) = self.replay_handover(&audit_id, &fingerprint, id).await? {
            return Ok(existing);
        }
        ensure_target_qualified(
            &self.db,
            self.require_rbac()?,
            req.target_user_id.trim(),
            &mut NoTransaction,
        )
        .await?;
        ensure_org_enabled(&self.db, req.target_org_unit_id.as_deref(), &mut NoTransaction).await?;
        let db = self.db.clone();
        let data_scope = self.data_scope.clone();
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let id = id.to_string();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let data_scope = data_scope.clone();
                let rbac = rbac.clone();
                let actor = actor.clone();
                let req = req.clone();
                let id = id.clone();
                let audit_id = audit_id.clone();
                let fingerprint = fingerprint.clone();
                let db = db.clone();
                Box::pin(async move {
                    apply_handover(
                        &db,
                        data_scope,
                        &rbac,
                        &id,
                        &req,
                        &actor,
                        &audit_id,
                        &fingerprint,
                        executor,
                    )
                    .await
                })
            })
            .await
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
        req.validate()?;
        let audit_id =
            format!("settlement-diff-handler-{}-{}-{}", actor.id(), id, req.idempotency_key.trim());
        let fingerprint = handler_fingerprint(actor.id(), id, &req)?;
        if let Some(existing) = self.replay_handler(&audit_id, &fingerprint, id).await? {
            return Ok(existing);
        }
        ensure_target_qualified(
            &self.db,
            self.require_rbac()?,
            req.target_user_id.trim(),
            &mut NoTransaction,
        )
        .await?;
        let db = self.db.clone();
        let data_scope = self.data_scope.clone();
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let id = id.to_string();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let data_scope = data_scope.clone();
                let rbac = rbac.clone();
                let actor = actor.clone();
                let req = req.clone();
                let id = id.clone();
                let audit_id = audit_id.clone();
                let fingerprint = fingerprint.clone();
                let db = db.clone();
                Box::pin(async move {
                    apply_handler(
                        &db,
                        data_scope,
                        &rbac,
                        &id,
                        &req,
                        &actor,
                        &audit_id,
                        &fingerprint,
                        executor,
                    )
                    .await
                })
            })
            .await
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
        let current =
            self.domain().access().require_statement(actor, "update", id, &mut NoTransaction).await?;
        list_candidates(&self.db, self.require_rbac()?, &current.prepared_by, &mut NoTransaction).await
    }

    async fn replay_handover(
        &self,
        audit_id: &str,
        fingerprint: &str,
        statement_id: &str,
    ) -> Result<Option<HandoverSettlementView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        ensure_fingerprint(audit.message.as_deref(), fingerprint, "结算交接")?;
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        Ok(Some(HandoverSettlementView {
            statement_id: statement.base.id,
            prepared_by: statement.prepared_by,
            business_org_unit_id: statement.business_org_unit_id,
            version: statement.base.version,
        }))
    }

    async fn replay_handler(
        &self,
        audit_id: &str,
        fingerprint: &str,
        statement_id: &str,
    ) -> Result<Option<ReassignSettlementDifferenceHandlerView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        ensure_fingerprint(audit.message.as_deref(), fingerprint, "差异处理人改派")?;
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        Ok(Some(ReassignSettlementDifferenceHandlerView {
            statement_id: statement.base.id.clone(),
            difference_handler_user_id: statement.difference_handler().to_string(),
            version: statement.base.version,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_handover(
    db: &mongodb::Database,
    data_scope: std::sync::Arc<dyn erp_supply::SettlementDataScopePort>,
    rbac: &SharedRbacService,
    id: &str,
    req: &HandoverSettlementRequest,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<HandoverSettlementView> {
    let access = erp_supply::service::supplier_settlement::SettlementAccess::new(db.clone(), data_scope);
    let mut statement = access.require_statement(actor, "update", id, executor).await?;
    if statement.base.version != req.expected_version {
        return Err(Error::ConflictError("结算单责任或版本已变化，请刷新后重试".into()));
    }
    ensure_target_qualified(db, rbac, req.target_user_id.trim(), executor).await?;
    ensure_org_enabled(db, req.target_org_unit_id.as_deref(), executor).await?;
    let next_org = req.target_org_unit_id.clone().filter(|value| !value.trim().is_empty());
    statement.handover(req.target_user_id.clone(), next_org)?;
    db.supplier_settlement_statements().update(&mut statement, executor).await?;
    write_audit(db, actor, audit_id, "supplier_settlement.handover", id, fingerprint, executor).await?;
    Ok(HandoverSettlementView {
        statement_id: statement.base.id,
        prepared_by: statement.prepared_by,
        business_org_unit_id: statement.business_org_unit_id,
        version: statement.base.version,
    })
}

#[allow(clippy::too_many_arguments)]
async fn apply_handler(
    db: &mongodb::Database,
    data_scope: std::sync::Arc<dyn erp_supply::SettlementDataScopePort>,
    rbac: &SharedRbacService,
    id: &str,
    req: &ReassignSettlementDifferenceHandlerRequest,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<ReassignSettlementDifferenceHandlerView> {
    let access = erp_supply::service::supplier_settlement::SettlementAccess::new(db.clone(), data_scope);
    let mut statement = access.require_statement(actor, "update", id, executor).await?;
    if statement.base.version != req.expected_version {
        return Err(Error::ConflictError("结算单责任或版本已变化，请刷新后重试".into()));
    }
    ensure_target_qualified(db, rbac, req.target_user_id.trim(), executor).await?;
    statement.reassign_difference_handler(req.target_user_id.clone())?;
    db.supplier_settlement_statements().update(&mut statement, executor).await?;
    write_audit(
        db,
        actor,
        audit_id,
        "supplier_settlement.reassign_difference_handler",
        id,
        fingerprint,
        executor,
    )
    .await?;
    Ok(ReassignSettlementDifferenceHandlerView {
        statement_id: statement.base.id.clone(),
        difference_handler_user_id: statement.difference_handler().to_string(),
        version: statement.base.version,
    })
}

async fn write_audit(
    db: &mongodb::Database,
    actor: &AuditActor,
    audit_id: &str,
    action: &str,
    resource_id: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let audit = actor.clone().resource_log_with_id(
        audit_id.to_string(),
        action,
        "supplier_settlement_statement",
        resource_id.to_string(),
        Some(format!("command_sha256={fingerprint}")),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

async fn ensure_target_qualified(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备结算维护资格".into()));
    };
    if !account.is_active_backoffice() {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备结算维护资格".into()));
    }
    let granted = PermissionSet::new(rbac.permissions(account.kind, target).await?);
    let required = Permission::parse("supplier_settlement_statement:update")?;
    if !granted.covers(&PermissionSet::new(vec![required])) {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备结算维护资格".into()));
    }
    Ok(())
}

async fn ensure_org_enabled(
    db: &mongodb::Database,
    target_org: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".into()));
    }
    Ok(())
}

async fn list_candidates(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    current: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<HandoverCandidateView>> {
    let accounts = db.accounts().list_by_kind(erp_core::AccountKind::Admin, executor).await?;
    let mut candidates = Vec::new();
    for account in accounts {
        if account.base.id == current || !account.is_active_backoffice() {
            continue;
        }
        let granted = PermissionSet::new(rbac.permissions(account.kind, &account.base.id).await?);
        let Ok(required) = Permission::parse("supplier_settlement_statement:update") else {
            continue;
        };
        if !granted.covers(&PermissionSet::new(vec![required])) {
            continue;
        }
        candidates.push(HandoverCandidateView {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
        });
    }
    candidates.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    Ok(candidates)
}

fn handover_fingerprint(actor_id: &str, id: &str, req: &HandoverSettlementRequest) -> Result<String> {
    Ok(digest(&[
        actor_id,
        id,
        req.target_user_id.trim(),
        req.target_org_unit_id.as_deref().unwrap_or(""),
        req.reason.trim(),
        &req.expected_version.to_string(),
        req.idempotency_key.trim(),
    ]))
}

fn handler_fingerprint(
    actor_id: &str,
    id: &str,
    req: &ReassignSettlementDifferenceHandlerRequest,
) -> Result<String> {
    Ok(digest(&[
        actor_id,
        id,
        req.target_user_id.trim(),
        req.reason.trim(),
        &req.expected_version.to_string(),
        req.idempotency_key.trim(),
    ]))
}

fn ensure_fingerprint(message: Option<&str>, expected: &str, command: &str) -> Result<()> {
    let Some(message) = message else {
        return Err(Error::ConflictError(format!("同一幂等键已用于不同的{command}")));
    };
    if !message.contains(expected) {
        return Err(Error::ConflictError(format!("同一幂等键已用于不同的{command}")));
    }
    Ok(())
}

fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}
