//! 供给维护人交接：目标资格、组织启用、幂等收据。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::AuditAction;
use erp_core::AccountKind;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, SharedRbacService, subject};
use erp_supply::entity::handover_receipt::{
    HANDOVER_ACTION, HANDOVER_LABEL, HANDOVER_RESOURCE, OfferingHandoverReceipt,
};
use erp_supply::repository::handover_receipt::OfferingHandoverReceiptExt;
use erp_supply::{HandoverCandidateView, HandoverSupplierOfferingRequest, HandoverSupplierOfferingView};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use crate::adapters::scoped_offering_service;
use crate::audit::{AuditedCommand, AuditedWrite, run_audited_event};
use crate::handover_common::{
    HANDOVER_AUDIT_FIELDS, HandoverEventFacts, ensure_org_enabled, handover_content, handover_context,
    trimmed_idempotency_key,
};
use crate::{Error, Result};

/// 显式交接供给维护人；目标须有效且具备 `supplier_offering:update`。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `id` - 供给 ID
/// * `req` - 交接请求
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回交接后责任视图。
///
/// # 错误
/// 账号失效、无维护资格、版本冲突或同幂等键异载荷时拒绝。
pub async fn handover_offering(
    db: Database,
    rbac: SharedRbacService,
    id: &str,
    req: HandoverSupplierOfferingRequest,
    actor: &AuditActor,
) -> Result<HandoverSupplierOfferingView> {
    req.validate()?;
    let key = trimmed_idempotency_key(&req.idempotency_key)?;
    let fingerprint = handover_fingerprint(actor.id(), id, &req, &key)?;
    let command = CommandReceipt::from_resource_parts(
        "offering-handover-",
        actor.id(),
        HANDOVER_ACTION,
        HANDOVER_RESOURCE,
        id,
        &key,
        [fingerprint],
    )?;
    if let Some(existing) = replay(&db, &rbac, &command, id, actor, &mut NoTransaction).await? {
        return Ok(existing);
    }
    ensure_target_qualified(&db, &rbac, req.target_user_id.trim(), &mut NoTransaction).await?;
    ensure_org_enabled(&db, req.target_org_unit_id.as_deref(), &mut NoTransaction).await?;
    let context = handover_context(
        actor,
        &command,
        AuditAction {
            code: HANDOVER_ACTION,
            resource_type: HANDOVER_RESOURCE,
            label: HANDOVER_LABEL,
            version: 1,
            allowed_fields: HANDOVER_AUDIT_FIELDS,
        },
    )?;
    let operation = OfferingHandoverCommand {
        db: db.clone(),
        rbac,
        id: id.to_string(),
        req,
        actor: actor.clone(),
        command,
        event_id: context.event_id().to_string(),
    };
    run_audited_event(&db, context, operation).await
}

/// 查询当前供给可交接的合格目标候选。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `id` - 供给 ID
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回有效且具备 `supplier_offering:update` 的账号。
///
/// # 错误
/// 对象不可见时拒绝。
pub async fn handover_candidates(
    db: Database,
    rbac: SharedRbacService,
    id: &str,
    actor: &AuditActor,
) -> Result<Vec<HandoverCandidateView>> {
    let offering = crate::adapters::offering_access(db.clone(), rbac.clone())
        .require_offering(actor, "update", id, &mut NoTransaction)
        .await?;
    let accounts = db.accounts().list_by_kind(erp_core::AccountKind::Admin, &mut NoTransaction).await?;
    let mut candidates = Vec::new();
    for account in accounts {
        if account.base.id == offering.maintainer_user_id || !account.is_active_backoffice() {
            continue;
        }
        if !account_can_maintain(&rbac, account.kind, &account.base.id).await? {
            continue;
        }
        candidates.push(HandoverCandidateView {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
        });
    }
    candidates.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.account.cmp(&right.account))
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    Ok(candidates)
}

/// 在原执行器中匹配独立回执，并重验当前访问读取当前责任视图。
///
/// # 参数
/// * `db` - 当前数据库。
/// * `rbac` - 当前权限快照。
/// * `command` - 当前请求的结构化命令身份。
/// * `id` - 领域目标。
/// * `actor` - 认证操作人。
/// * `executor` - 当前查证或事务执行器。
/// # 返回
/// 无回执时返回 None，命中时返回当前责任视图。
/// # 错误
/// 指纹冲突、回执损坏、权限拒绝或持久化失败时返回错误。
async fn replay(
    db: &Database,
    rbac: &SharedRbacService,
    command: &CommandReceipt,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<HandoverSupplierOfferingView>> {
    let Some(receipt) =
        db.offering_handover_receipts().find_by_id_including_deleted(command.id(), executor).await?
    else {
        return Ok(None);
    };
    receipt.ensure_matches(command)?;
    let offering = crate::adapters::offering_access(db.clone(), rbac.clone())
        .require_offering(actor, "update", id, executor)
        .await?;
    Ok(Some(HandoverSupplierOfferingView {
        offering_id: offering.base.id,
        maintainer_user_id: offering.maintainer_user_id,
        business_org_unit_id: offering.business_org_unit_id,
        version: offering.base.version,
    }))
}

/// 命名 Process 的执行边界，持有首次请求和安全审计关联。
struct OfferingHandoverCommand {
    db: Database,
    rbac: SharedRbacService,
    id: String,
    req: HandoverSupplierOfferingRequest,
    actor: AuditActor,
    command: CommandReceipt,
    event_id: String,
}

#[async_trait]
impl AuditedCommand for OfferingHandoverCommand {
    type Output = HandoverSupplierOfferingView;

    /// 同 Executor 查证、资格校验、交接和回执；重放不追加成功事件。
    ///
    /// # 参数
    /// * `executor` - 查证、交接与回执共用的调用方执行器。
    ///
    /// # 返回
    /// 已有匹配回执时返回 `AuditedWrite::Replayed`，不追加成功事件。首次成功返回 `AuditedWrite::Fresh`，含交接事件和当前供给视图。
    ///
    /// # 错误
    /// 回执不匹配、目标账号或组织校验失败、供给不可更新、交接写入、回执构造或保存失败时返回对应错误。
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        if let Some(view) =
            replay(&self.db, &self.rbac, &self.command, &self.id, &self.actor, executor).await?
        {
            return Ok(AuditedWrite::Replayed(view));
        }
        ensure_target_qualified(&self.db, &self.rbac, self.req.target_user_id.trim(), executor).await?;
        ensure_org_enabled(&self.db, self.req.target_org_unit_id.as_deref(), executor).await?;
        let before = crate::adapters::offering_access(self.db.clone(), self.rbac.clone())
            .require_offering(&self.actor, "update", &self.id, executor)
            .await?;
        let view = scoped_offering_service(self.db.clone(), self.rbac.clone())
            .apply_offering_handover(&self.id, &self.req, &self.actor, executor)
            .await?;
        let receipt = OfferingHandoverReceipt::new(&self.command, view.clone(), self.event_id.clone())?;
        self.db.offering_handover_receipts().create(&receipt, executor).await?;
        Ok(AuditedWrite::Fresh {
            content: handover_content(HandoverEventFacts {
                target_id: view.offering_id.clone(),
                target_number: Some(before.supplier_sku_code),
                responsibility_changed: before.maintainer_user_id != view.maintainer_user_id,
                organization_changed: before.business_org_unit_id != view.business_org_unit_id,
            }),
            result: view,
        })
    }
}

/// 校验目标账号有效且具备供给维护资格。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `target` - 目标账号
/// * `executor` - 调用方执行器
///
/// # 返回
/// 合格时成功。
///
/// # 错误
/// 账号不存在、失效或无 `supplier_offering:update` 时拒绝。
async fn ensure_target_qualified(
    db: &Database,
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备供给维护资格".into()));
    };
    if !account.is_active_backoffice() || !account_can_maintain(rbac, account.kind, target).await? {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备供给维护资格".into()));
    }
    Ok(())
}

/// 判断账号是否具备 `supplier_offering:update`。
///
/// # 参数
/// * `rbac` - RBAC 快照
/// * `kind` - 账号类型；与路由鉴权一起组成 Casbin 主体
/// * `user_id` - 账号 ID
///
/// # 返回
/// 具备维护资格时为 true。
///
/// # 错误
/// RBAC 判定失败时拒绝。
///
/// # 关键业务约束
/// 主体必须是 `user:{kind}:{id}`。裸账号 ID 对不上已发布策略，交接会把合格维护人全部拒绝。
async fn account_can_maintain(rbac: &SharedRbacService, kind: AccountKind, user_id: &str) -> Result<bool> {
    let permission = Permission::parse("supplier_offering:update").map_err(Error::from)?;
    Ok(rbac.enforce(&maintainer_casbin_subject(kind, user_id), &permission).await?)
}

/// 供给维护资格使用与路由鉴权相同的 Casbin 主体。
///
/// # 参数
/// * `kind` - 账号类型
/// * `user_id` - 账号 ID
///
/// # 返回
/// 返回 `user:{kind}:{id}`。
///
/// # 错误
/// 无。
fn maintainer_casbin_subject(kind: AccountKind, user_id: &str) -> String {
    subject(kind, user_id)
}

/// 计算交接命令指纹，供幂等回放比对。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `offering_id` - 供给 ID
/// * `req` - 交接请求
/// * `key` - 幂等键
///
/// # 返回
/// 返回命令序列化指纹。
///
/// # 错误
/// 序列化失败时拒绝。
fn handover_fingerprint(
    actor_id: &str,
    offering_id: &str,
    req: &HandoverSupplierOfferingRequest,
    key: &str,
) -> Result<String> {
    serde_json::to_string(&(
        actor_id,
        offering_id,
        req.target_user_id.trim(),
        req.target_org_unit_id.as_deref().map(str::trim),
        req.reason.trim(),
        req.expected_version,
        key,
    ))
    .map_err(|error| Error::Internal(format!("供给交接命令序列化失败: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> HandoverSupplierOfferingRequest {
        HandoverSupplierOfferingRequest {
            target_user_id: "user-2".into(),
            target_org_unit_id: Some("org-2".into()),
            reason: "交接维护人".into(),
            expected_version: 3,
            idempotency_key: "key-1".into(),
        }
    }

    #[test]
    fn handover_fingerprint_is_payload_sensitive() {
        let req = request();
        let first = handover_fingerprint("actor", "offering-1", &req, "key-1").unwrap();
        assert_eq!(first, handover_fingerprint("actor", "offering-1", &req, "key-1").unwrap());
        let mut other = req.clone();
        other.target_user_id = "user-3".into();
        assert_ne!(first, handover_fingerprint("actor", "offering-1", &other, "key-1").unwrap());
        let mut other_key = req.clone();
        other_key.idempotency_key = "key-2".into();
        assert_ne!(first, handover_fingerprint("actor", "offering-1", &other_key, "key-2").unwrap());
    }

    #[test]
    fn maintainer_subject_includes_account_kind() {
        let user_id = "user-1";
        let casbin_subject = maintainer_casbin_subject(AccountKind::Admin, user_id);
        assert_eq!(casbin_subject, format!("user:admin:{user_id}"));
        assert_ne!(casbin_subject, user_id);
    }

    #[test]
    fn structured_handover_keeps_existing_payload_normalization() {
        let req = request();
        let fp = handover_fingerprint("actor", "offering-1", &req, "key-1").unwrap();
        let command = CommandReceipt::from_resource_parts(
            "offering-handover-",
            "actor",
            HANDOVER_ACTION,
            HANDOVER_RESOURCE,
            "offering-1",
            "key-1",
            [fp],
        )
        .unwrap();
        let mut normalized = req.clone();
        normalized.target_user_id = "  user-2  ".into();
        let equivalent = handover_fingerprint("actor", "offering-1", &normalized, "key-1").unwrap();
        let equivalent = CommandReceipt::from_resource_parts(
            "offering-handover-",
            "actor",
            HANDOVER_ACTION,
            HANDOVER_RESOURCE,
            "offering-1",
            "key-1",
            [equivalent],
        )
        .unwrap();
        let receipt = OfferingHandoverReceipt::new(
            &command,
            HandoverSupplierOfferingView {
                offering_id: "offering-1".into(),
                maintainer_user_id: "user-2".into(),
                business_org_unit_id: "org-2".into(),
                version: 4,
            },
            "event".into(),
        )
        .unwrap();
        assert!(receipt.ensure_matches(&equivalent).is_ok());
    }
}
