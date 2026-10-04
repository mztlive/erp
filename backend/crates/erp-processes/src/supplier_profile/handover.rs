//! 供应商维护人与能力负责人显式交接。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::{AuditAction, BusinessEventContent};
use erp_core::ids::{SupplierAccountId, SupplierCapabilityRevisionId};
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, PermissionSet, SharedRbacService};
use erp_supplier::entity::handover_receipt::{
    CAPABILITY_HANDOVER_ACTION, SUPPLIER_HANDOVER_ACTION, SupplierHandoverReceipt, SupplierHandoverResult,
};
use erp_supplier::repository::handover_receipt::SupplierHandoverReceiptExt;
use erp_supplier::{
    HandoverCandidateView, HandoverSupplierCapabilityRequest, HandoverSupplierCapabilityView,
    HandoverSupplierRequest, HandoverSupplierView, SupplierExt, capability_handover_fingerprint,
    supplier_handover_fingerprint,
};
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::SupplierProfileService;
use crate::audit::{AuditedCommand, AuditedWrite, run_audited_event};
use crate::handover_common::{
    HANDOVER_AUDIT_FIELDS, HandoverEventFacts, ensure_org_enabled, handover_content, handover_context,
};
use crate::{Error, Result};

/// 供应商交接明确登记的安全动作。
const SUPPLIER_ACTION: AuditAction = AuditAction {
    code: SUPPLIER_HANDOVER_ACTION,
    resource_type: "supplier",
    label: "供应商交接",
    version: 1,
    allowed_fields: HANDOVER_AUDIT_FIELDS,
};
/// 能力交接明确登记的安全动作，与供应商账户目标区分。
const CAPABILITY_ACTION: AuditAction = AuditAction {
    code: CAPABILITY_HANDOVER_ACTION,
    resource_type: "supplier_capability",
    label: "能力负责人交接",
    version: 1,
    allowed_fields: HANDOVER_AUDIT_FIELDS,
};

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
        let fingerprint = supplier_handover_fingerprint(actor.id(), id, &req)?;
        let command = CommandReceipt::from_resource_parts(
            "supplier-handover-",
            actor.id(),
            SUPPLIER_HANDOVER_ACTION,
            "supplier",
            id,
            &req.idempotency_key,
            [fingerprint],
        )?;
        let rbac = self.require_rbac()?.clone();
        if let Some(view) = replay_supplier(&self.db, &rbac, &command, id, actor, &mut NoTransaction).await? {
            return Ok(view);
        }
        self.ensure_target_qualified(req.target_user_id.trim(), &mut NoTransaction).await?;
        self.ensure_org_enabled(req.target_org_unit_id.as_deref(), &mut NoTransaction).await?;
        let context = handover_context(actor, &command, SUPPLIER_ACTION)?;
        let operation = SupplierHandoverCommand {
            db: self.db.clone(),
            rbac,
            actor: actor.clone(),
            command,
            event_id: context.event_id().to_string(),
            request: HandoverRequest::Supplier { id: id.to_string(), req },
        };
        match run_audited_event(&self.db, context, operation).await? {
            HandoverOutput::Supplier(view) => Ok(view),
            HandoverOutput::Capability(_) => Err(Error::Internal("供应商交接结果类型无效".into())),
        }
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
        let fingerprint = capability_handover_fingerprint(actor.id(), capability_id, &req)?;
        let command = CommandReceipt::from_resource_parts(
            "supplier-capability-handover-",
            actor.id(),
            CAPABILITY_HANDOVER_ACTION,
            "supplier_capability",
            capability_id,
            &req.idempotency_key,
            [supplier_id.to_string(), fingerprint],
        )?;
        let rbac = self.require_rbac()?.clone();
        if let Some(view) = replay_capability(
            &self.db,
            &rbac,
            &command,
            supplier_id,
            capability_id,
            actor,
            &mut NoTransaction,
        )
        .await?
        {
            return Ok(view);
        }
        self.ensure_target_qualified(req.target_user_id.trim(), &mut NoTransaction).await?;
        let context = handover_context(actor, &command, CAPABILITY_ACTION)?;
        let operation = SupplierHandoverCommand {
            db: self.db.clone(),
            rbac,
            actor: actor.clone(),
            command,
            event_id: context.event_id().to_string(),
            request: HandoverRequest::Capability {
                supplier_id: supplier_id.to_string(),
                capability_id: capability_id.to_string(),
                req,
            },
        };
        match run_audited_event(&self.db, context, operation).await? {
            HandoverOutput::Capability(view) => Ok(view),
            HandoverOutput::Supplier(_) => Err(Error::Internal("能力交接结果类型无效".into())),
        }
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
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(HandoverSupplierView, BusinessEventContent)> {
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
    let old_owner = account.maintainer_user_id.clone();
    let old_org = account.business_org_unit_id.clone();
    account.handover(target.to_string(), next_org, actor.id())?;
    db.supplier_accounts().update(&mut account, executor).await?;
    let content = handover_content(HandoverEventFacts {
        target_id: account.base.id.clone(),
        target_number: Some(account.supplier_no.clone()),
        responsibility_changed: old_owner != account.maintainer_user_id,
        organization_changed: old_org != account.business_org_unit_id,
    });
    Ok((
        HandoverSupplierView {
            supplier_id: account.base.id,
            maintainer_user_id: account.maintainer_user_id,
            business_org_unit_id: account.business_org_unit_id,
            version: account.base.version,
        },
        content,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn apply_capability_handover(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    supplier_id: &str,
    capability_id: &str,
    req: &HandoverSupplierCapabilityRequest,
    target: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(HandoverSupplierCapabilityView, BusinessEventContent)> {
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
    let old_owner = capability.owner_user_id.clone();
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
    let content = handover_content(HandoverEventFacts {
        target_id: capability.base.id.clone(),
        target_number: None,
        responsibility_changed: old_owner != capability.owner_user_id,
        organization_changed: false,
    });
    Ok((
        HandoverSupplierCapabilityView {
            supplier_id: supplier_id.to_string(),
            capability_id: capability.base.id,
            owner_user_id: capability.owner_user_id,
            version: capability.base.version,
        },
        content,
    ))
}

/// 当前命令可提交的两类交接，目标类型由 enum 和核心身份共同约束。
enum HandoverRequest {
    Supplier { id: String, req: HandoverSupplierRequest },
    Capability { supplier_id: String, capability_id: String, req: HandoverSupplierCapabilityRequest },
}

/// 供应商命名 Process 的原子执行边界。
struct SupplierHandoverCommand {
    db: mongodb::Database,
    rbac: SharedRbacService,
    actor: AuditActor,
    command: CommandReceipt,
    event_id: String,
    request: HandoverRequest,
}

#[async_trait]
impl AuditedCommand for SupplierHandoverCommand {
    type Output = HandoverOutput;

    /// 同执行器查证和交接，成功回执与正式事实共同提交。
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        let outcome = match &self.request {
            HandoverRequest::Supplier { id, req } => self.execute_supplier(id, req, executor).await?,
            HandoverRequest::Capability { supplier_id, capability_id, req } => {
                self.execute_capability(supplier_id, capability_id, req, executor).await?
            },
        };
        match outcome {
            AuditedWrite::Replayed(view) => Ok(AuditedWrite::Replayed(view)),
            AuditedWrite::Fresh { result: output, content } => {
                let result = match &output {
                    HandoverOutput::Supplier(view) => SupplierHandoverResult::Supplier(view.clone()),
                    HandoverOutput::Capability(view) => SupplierHandoverResult::Capability(view.clone()),
                };
                let receipt = SupplierHandoverReceipt::new(&self.command, result, self.event_id.clone())?;
                self.db.supplier_handover_receipts().create(&receipt, executor).await?;
                Ok(AuditedWrite::Fresh { result: output, content })
            },
        }
    }
}

impl SupplierHandoverCommand {
    /// 执行供应商分支；回执命中时只返回当前授权视图。
    async fn execute_supplier(
        &self,
        id: &str,
        req: &HandoverSupplierRequest,
        executor: &mut dyn Executor,
    ) -> Result<AuditedWrite<HandoverOutput>> {
        if let Some(view) =
            replay_supplier(&self.db, &self.rbac, &self.command, id, &self.actor, executor).await?
        {
            return Ok(AuditedWrite::Replayed(HandoverOutput::Supplier(view)));
        }
        let (view, content) = apply_supplier_handover(
            &self.db,
            &self.rbac,
            id,
            req,
            req.target_user_id.trim(),
            &self.actor,
            executor,
        )
        .await?;
        Ok(AuditedWrite::Fresh { result: HandoverOutput::Supplier(view), content })
    }

    /// 执行能力分支；原请求始终绑定供应商和能力目标。
    async fn execute_capability(
        &self,
        supplier_id: &str,
        capability_id: &str,
        req: &HandoverSupplierCapabilityRequest,
        executor: &mut dyn Executor,
    ) -> Result<AuditedWrite<HandoverOutput>> {
        if let Some(view) = replay_capability(
            &self.db,
            &self.rbac,
            &self.command,
            supplier_id,
            capability_id,
            &self.actor,
            executor,
        )
        .await?
        {
            return Ok(AuditedWrite::Replayed(HandoverOutput::Capability(view)));
        }
        let (view, content) = apply_capability_handover(
            &self.db,
            &self.rbac,
            supplier_id,
            capability_id,
            req,
            req.target_user_id.trim(),
            &self.actor,
            executor,
        )
        .await?;
        Ok(AuditedWrite::Fresh { result: HandoverOutput::Capability(view), content })
    }
}

/// Process 内部结果类型，入口严格取各自分支。
enum HandoverOutput {
    Supplier(HandoverSupplierView),
    Capability(HandoverSupplierCapabilityView),
}

/// 独立回执命中后重验当前供应商访问，返回当前责任视图。
async fn replay_supplier(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    command: &CommandReceipt,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<HandoverSupplierView>> {
    let Some(receipt) =
        db.supplier_handover_receipts().find_by_id_including_deleted(command.id(), executor).await?
    else {
        return Ok(None);
    };
    receipt.ensure_matches(command)?;
    crate::adapters::supplier_access(db.clone(), rbac.clone())
        .require_with(actor, "update", id, executor)
        .await?;
    let account = db
        .supplier()
        .account(&SupplierAccountId::new(id), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    Ok(Some(HandoverSupplierView {
        supplier_id: account.base.id,
        maintainer_user_id: account.maintainer_user_id,
        business_org_unit_id: account.business_org_unit_id,
        version: account.base.version,
    }))
}

/// 能力回放保留当前访问和供应商所属核验，不跨目标返回结果。
#[allow(clippy::too_many_arguments)]
async fn replay_capability(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    command: &CommandReceipt,
    supplier_id: &str,
    capability_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<HandoverSupplierCapabilityView>> {
    let Some(receipt) =
        db.supplier_handover_receipts().find_by_id_including_deleted(command.id(), executor).await?
    else {
        return Ok(None);
    };
    receipt.ensure_matches(command)?;
    crate::adapters::supplier_access(db.clone(), rbac.clone())
        .require_with(actor, "update", supplier_id, executor)
        .await?;
    let capability = db
        .supplier_capabilities()
        .find_by_id(capability_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供给能力不存在".into()))?;
    if capability.supplier_id.as_ref() != supplier_id {
        return Err(Error::NotFound("供给能力不存在".into()));
    }
    Ok(Some(HandoverSupplierCapabilityView {
        supplier_id: supplier_id.to_string(),
        capability_id: capability.base.id,
        owner_user_id: capability.owner_user_id,
        version: capability.base.version,
    }))
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
