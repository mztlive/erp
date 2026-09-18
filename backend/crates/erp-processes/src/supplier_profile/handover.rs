//! 供应商维护人与能力负责人显式交接。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::{SupplierAccountId, SupplierCapabilityRevisionId};
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{AccessControlExt, Permission, PermissionSet, SharedRbacService};
use erp_supplier::{
    HandoverCandidateView, HandoverSupplierCapabilityRequest, HandoverSupplierCapabilityView,
    HandoverSupplierRequest, HandoverSupplierView, SupplierExt, capability_handover_audit_id,
    capability_handover_fingerprint, supplier_handover_audit_id, supplier_handover_audit_message,
    supplier_handover_fingerprint, supplier_handover_fingerprint_matches,
};
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::SupplierProfileService;
use crate::{Error, Result};

impl SupplierProfileService {
    /// 显式交接供应商整体维护人；开放审批任务不改派。
    ///
    /// # 参数
    /// * `id` - 供应商 ID
    /// * `req` - 交接请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回交接后的维护人与业务组织。
    ///
    /// # 错误
    /// 版本冲突、目标不合格、范围不足或幂等键载荷不一致时拒绝。
    pub async fn handover_supplier(
        &self,
        id: &str,
        req: HandoverSupplierRequest,
        actor: &AuditActor,
    ) -> Result<HandoverSupplierView> {
        req.validate()?;
        let target = req.target_user_id.trim().to_string();
        let reason = req.reason.trim().to_string();
        let audit_id = supplier_handover_audit_id(actor.id(), id, &req.idempotency_key);
        let fingerprint = supplier_handover_fingerprint(actor.id(), id, &req)?;
        if let Some(existing) = self.replay_supplier_handover(&audit_id, &fingerprint, id).await? {
            return Ok(existing);
        }
        self.ensure_target_qualified(&target, &mut NoTransaction).await?;
        self.ensure_org_enabled(req.target_org_unit_id.as_deref(), &mut NoTransaction).await?;
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let db = self.db.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    apply_supplier_handover(
                        &db,
                        &rbac,
                        &id,
                        &req,
                        &target,
                        &reason,
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

    /// 显式交接供给能力负责人；开放审批任务不改派。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商 ID
    /// * `capability_id` - 供给能力 ID
    /// * `req` - 交接请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回交接后的能力负责人。
    ///
    /// # 错误
    /// 版本冲突、目标不合格、范围不足或幂等键载荷不一致时拒绝。
    pub async fn handover_capability(
        &self,
        supplier_id: &str,
        capability_id: &str,
        req: HandoverSupplierCapabilityRequest,
        actor: &AuditActor,
    ) -> Result<HandoverSupplierCapabilityView> {
        req.validate()?;
        let target = req.target_user_id.trim().to_string();
        let reason = req.reason.trim().to_string();
        let audit_id = capability_handover_audit_id(actor.id(), capability_id, &req.idempotency_key);
        let fingerprint = capability_handover_fingerprint(actor.id(), capability_id, &req)?;
        if let Some(existing) =
            self.replay_capability_handover(&audit_id, &fingerprint, supplier_id, capability_id).await?
        {
            return Ok(existing);
        }
        self.ensure_target_qualified(&target, &mut NoTransaction).await?;
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let supplier_id = supplier_id.to_string();
        let capability_id = capability_id.to_string();
        let db = self.db.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    apply_capability_handover(
                        &db,
                        &rbac,
                        &supplier_id,
                        &capability_id,
                        &req,
                        &target,
                        &reason,
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
    /// * `id` - 供应商 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回不含当前维护人的合格后台账号。
    ///
    /// # 错误
    /// 供应商不存在、范围不足或仓储失败时拒绝。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<HandoverCandidateView>> {
        crate::adapters::supplier_access(self.db.clone(), self.require_rbac()?.clone())
            .require(actor, "update", id)
            .await?;
        let current = self
            .db
            .supplier()
            .account(&SupplierAccountId::new(id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        list_candidates(&self.db, self.require_rbac()?, &current.maintainer_user_id, &mut NoTransaction).await
    }

    async fn replay_supplier_handover(
        &self,
        audit_id: &str,
        fingerprint: &str,
        supplier_id: &str,
    ) -> Result<Option<HandoverSupplierView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        if !supplier_handover_fingerprint_matches(audit.message.as_deref(), fingerprint) {
            return Err(Error::ConflictError("同一幂等键已用于不同的供应商交接".into()));
        }
        let account = self
            .db
            .supplier()
            .account(&SupplierAccountId::new(supplier_id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        Ok(Some(HandoverSupplierView {
            supplier_id: account.base.id,
            maintainer_user_id: account.maintainer_user_id,
            business_org_unit_id: account.business_org_unit_id,
            version: account.base.version,
        }))
    }

    async fn replay_capability_handover(
        &self,
        audit_id: &str,
        fingerprint: &str,
        supplier_id: &str,
        capability_id: &str,
    ) -> Result<Option<HandoverSupplierCapabilityView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        if !supplier_handover_fingerprint_matches(audit.message.as_deref(), fingerprint) {
            return Err(Error::ConflictError("同一幂等键已用于不同的能力交接".into()));
        }
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_id(capability_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给能力不存在".to_string()))?;
        Ok(Some(HandoverSupplierCapabilityView {
            supplier_id: supplier_id.to_string(),
            capability_id: capability.base.id,
            owner_user_id: capability.owner_user_id,
            version: capability.base.version,
        }))
    }

    async fn ensure_target_qualified(&self, target: &str, executor: &mut dyn Executor) -> Result<()> {
        ensure_target_qualified(&self.db, self.require_rbac()?, target, executor).await
    }

    async fn ensure_org_enabled(&self, target_org: Option<&str>, executor: &mut dyn Executor) -> Result<()> {
        ensure_org_enabled(&self.db, target_org, executor).await
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_supplier_handover(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    id: &str,
    req: &HandoverSupplierRequest,
    target: &str,
    reason: &str,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<HandoverSupplierView> {
    crate::adapters::supplier_access(db.clone(), rbac.clone())
        .require_with(actor, "update", id, executor)
        .await?;
    ensure_target_qualified(db, rbac, target, executor).await?;
    ensure_org_enabled(db, req.target_org_unit_id.as_deref(), executor).await?;
    let mut account = db
        .supplier()
        .account(&SupplierAccountId::new(id), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    if account.base.version != req.expected_version {
        return Err(Error::ConflictError("供应商责任或版本已变化，请刷新后重试".into()));
    }
    let next_org = req
        .target_org_unit_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    account.handover(target.to_string(), next_org, actor.id())?;
    db.supplier_accounts().update(&mut account, executor).await?;
    write_audit(
        db,
        actor,
        audit_id,
        "supplier.handover",
        "supplier",
        &account.base.id,
        fingerprint,
        target,
        reason,
        executor,
    )
    .await?;
    Ok(HandoverSupplierView {
        supplier_id: account.base.id,
        maintainer_user_id: account.maintainer_user_id,
        business_org_unit_id: account.business_org_unit_id,
        version: account.base.version,
    })
}

#[allow(clippy::too_many_arguments)]
async fn apply_capability_handover(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    supplier_id: &str,
    capability_id: &str,
    req: &HandoverSupplierCapabilityRequest,
    target: &str,
    reason: &str,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<HandoverSupplierCapabilityView> {
    crate::adapters::supplier_access(db.clone(), rbac.clone())
        .require_with(actor, "update", supplier_id, executor)
        .await?;
    ensure_target_qualified(db, rbac, target, executor).await?;
    let mut capability = db
        .supplier_capabilities()
        .find_by_id(capability_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供给能力不存在".to_string()))?;
    if capability.supplier_id.to_string() != supplier_id {
        return Err(Error::NotFound("供给能力不存在".into()));
    }
    if capability.base.version != req.expected_version {
        return Err(Error::ConflictError("能力责任或版本已变化，请刷新后重试".into()));
    }
    capability.handover(target.to_string(), actor.id())?;
    let revision_no = db
        .supplier()
        .next_capability_revision_no(&capability.supplier_id, capability.capability_code, executor)
        .await?;
    let revision_id = SupplierCapabilityRevisionId::new(id_generator::next_id());
    capability.stable.current_revision_id = Some(revision_id.to_string());
    let revision = capability.snapshot_revision(revision_id, revision_no)?;
    db.supplier_capability_revisions().create(&revision, executor).await?;
    db.supplier_capabilities().update(&mut capability, executor).await?;
    write_audit(
        db,
        actor,
        audit_id,
        "supplier_capability.handover",
        "supplier_capability",
        &capability.base.id,
        fingerprint,
        target,
        reason,
        executor,
    )
    .await?;
    Ok(HandoverSupplierCapabilityView {
        supplier_id: supplier_id.to_string(),
        capability_id: capability.base.id,
        owner_user_id: capability.owner_user_id,
        version: capability.base.version,
    })
}

#[allow(clippy::too_many_arguments)]
async fn write_audit(
    db: &mongodb::Database,
    actor: &AuditActor,
    audit_id: &str,
    action: &str,
    resource: &str,
    resource_id: &str,
    fingerprint: &str,
    target: &str,
    reason: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let audit = actor.clone().resource_log_with_id(
        audit_id.to_string(),
        action,
        resource,
        resource_id.to_string(),
        Some(supplier_handover_audit_message(fingerprint, target, reason)),
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
    if account_can_maintain(db, rbac, target, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("目标账号不存在、已失效或不具备供应商维护资格".into()))
}

async fn account_can_maintain(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Ok(false);
    };
    if !account.is_active_backoffice() {
        return Ok(false);
    }
    let granted = PermissionSet::new(rbac.permissions(account.kind, target).await?);
    let Ok(required) = Permission::parse("supplier:update") else {
        return Ok(false);
    };
    Ok(granted.covers(&PermissionSet::new(vec![required])))
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
        if !account_can_maintain(db, rbac, &account.base.id, executor).await? {
            continue;
        }
        candidates.push(HandoverCandidateView {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
        });
    }
    candidates.sort_by(|left, right| {
        left.display_name.cmp(&right.display_name).then_with(|| left.account.cmp(&right.account))
    });
    Ok(candidates)
}
