//! 商品维护人交接：目标资格、组织启用、幂等收据。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::AuditAction;
use erp_catalog::entity::handover_receipt::{
    HANDOVER_ACTION, HANDOVER_LABEL, HANDOVER_RESOURCE, ProductHandoverReceipt,
};
use erp_catalog::repository::handover_receipt::ProductHandoverReceiptExt;
use erp_catalog::{HandoverCandidateView, HandoverProductRequest, HandoverProductView};
use erp_core::AccountKind;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, SharedRbacService, subject};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use crate::adapters::scoped_catalog_service;
use crate::audit::{AuditedCommand, AuditedWrite, run_audited_event};
use crate::handover_common::{
    HANDOVER_AUDIT_FIELDS, HandoverEventFacts, ensure_org_enabled, handover_content, handover_context,
    trimmed_idempotency_key,
};
use crate::{Error, Result};

/// 显式交接商品维护人；目标须有效且具备 `product:update`。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `id` - 商品 ID
/// * `req` - 交接请求
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回交接后责任视图。
///
/// # 错误
/// 账号失效、无维护资格、版本冲突或同幂等键异载荷时拒绝。
pub async fn handover_product(
    db: Database,
    rbac: SharedRbacService,
    id: &str,
    req: HandoverProductRequest,
    actor: &AuditActor,
) -> Result<HandoverProductView> {
    req.validate()?;
    let key = trimmed_idempotency_key(&req.idempotency_key)?;
    let fingerprint = handover_fingerprint(actor.id(), id, &req, &key)?;
    let command = CommandReceipt::from_resource_parts(
        "product-handover-",
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
    let operation = ProductHandoverCommand {
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

/// 查询当前商品可交接的合格目标候选。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `id` - 商品 ID
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回有效且具备 `product:update` 的账号。
///
/// # 错误
/// 对象不可见时拒绝。
pub async fn handover_candidates(
    db: Database,
    rbac: SharedRbacService,
    id: &str,
    actor: &AuditActor,
) -> Result<Vec<HandoverCandidateView>> {
    let product = crate::adapters::catalog_access(db.clone(), rbac.clone())
        .require_product(actor, "update", id, &mut NoTransaction)
        .await?;
    let accounts = db.accounts().list_by_kind(erp_core::AccountKind::Admin, &mut NoTransaction).await?;
    let mut candidates = Vec::new();
    for account in accounts {
        if account.base.id == product.maintainer_user_id || !account.is_active_backoffice() {
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
) -> Result<Option<HandoverProductView>> {
    let Some(receipt) =
        db.product_handover_receipts().find_by_id_including_deleted(command.id(), executor).await?
    else {
        return Ok(None);
    };
    receipt.ensure_matches(command)?;
    let product = crate::adapters::catalog_access(db.clone(), rbac.clone())
        .require_product(actor, "update", id, executor)
        .await?;
    Ok(Some(HandoverProductView {
        product_id: product.base.id,
        maintainer_user_id: product.maintainer_user_id,
        business_org_unit_id: product.business_org_unit_id,
        version: product.base.version,
    }))
}

/// 命名 Process 的执行边界，持有首次请求和安全审计关联。
struct ProductHandoverCommand {
    db: Database,
    rbac: SharedRbacService,
    id: String,
    req: HandoverProductRequest,
    actor: AuditActor,
    command: CommandReceipt,
    event_id: String,
}

#[async_trait]
impl AuditedCommand for ProductHandoverCommand {
    type Output = HandoverProductView;

    /// 同 Executor 查证、资格校验、交接和回执；重放不追加成功事件。
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        if let Some(view) =
            replay(&self.db, &self.rbac, &self.command, &self.id, &self.actor, executor).await?
        {
            return Ok(AuditedWrite::Replayed(view));
        }
        ensure_target_qualified(&self.db, &self.rbac, self.req.target_user_id.trim(), executor).await?;
        ensure_org_enabled(&self.db, self.req.target_org_unit_id.as_deref(), executor).await?;
        let before = crate::adapters::catalog_access(self.db.clone(), self.rbac.clone())
            .require_product(&self.actor, "update", &self.id, executor)
            .await?;
        let view = scoped_catalog_service(self.db.clone(), self.rbac.clone())
            .apply_product_handover(&self.id, &self.req, &self.actor, executor)
            .await?;
        let receipt = ProductHandoverReceipt::new(&self.command, view.clone(), self.event_id.clone())?;
        self.db.product_handover_receipts().create(&receipt, executor).await?;
        Ok(AuditedWrite::Fresh {
            content: handover_content(HandoverEventFacts {
                target_id: view.product_id.clone(),
                target_number: Some(before.product_no),
                responsibility_changed: before.maintainer_user_id != view.maintainer_user_id,
                organization_changed: before.business_org_unit_id != view.business_org_unit_id,
            }),
            result: view,
        })
    }
}

/// 校验目标账号有效且具备商品维护资格。
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
/// 账号不存在、失效或无 `product:update` 时拒绝。
async fn ensure_target_qualified(
    db: &Database,
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备商品维护资格".into()));
    };
    if !account.is_active_backoffice() || !account_can_maintain(rbac, account.kind, target).await? {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备商品维护资格".into()));
    }
    Ok(())
}

/// 判断账号是否具备 `product:update`。
///
/// # 参数
/// * `rbac` - RBAC 快照
/// * `kind` - 目标账号类型
/// * `user_id` - 目标账号 ID
///
/// # 返回
/// 具备维护资格时为 true。
///
/// # 错误
/// RBAC 判定失败时拒绝。
async fn account_can_maintain(rbac: &SharedRbacService, kind: AccountKind, user_id: &str) -> Result<bool> {
    let (subject, permission) = maintainer_authorization(kind, user_id)?;
    Ok(rbac.enforce(&subject, &permission).await?)
}

/// 构造商品维护资格所需的规范主体和固定权限。
///
/// # 参数
/// * `kind` - 已读取的目标账号类型
/// * `user_id` - 已读取的目标账号 ID
///
/// # 返回
/// 返回与 HTTP 鉴权一致的 `user:{kind}:{id}` 主体及 `product:update`。
///
/// # 错误
/// 固定权限无法解析时返回错误，不降级为通配权限。
fn maintainer_authorization(kind: AccountKind, user_id: &str) -> Result<(String, Permission)> {
    let permission = Permission::parse("product:update").map_err(Error::from)?;
    Ok((subject(kind, user_id), permission))
}

/// 计算交接命令指纹，供幂等回放比对。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `product_id` - 商品 ID
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
    product_id: &str,
    req: &HandoverProductRequest,
    key: &str,
) -> Result<String> {
    serde_json::to_string(&(
        actor_id,
        product_id,
        req.target_user_id.trim(),
        req.target_org_unit_id.as_deref().map(str::trim),
        req.reason.trim(),
        req.expected_version,
        key,
    ))
    .map_err(|error| Error::Internal(format!("商品交接命令序列化失败: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 维护资格请求使用规范主体和商品更新权限，避免裸 ID 导致全部拒绝。
    #[test]
    fn maintainer_authorization_uses_canonical_subject_and_product_update() {
        let (subject, permission) = maintainer_authorization(AccountKind::Admin, "maintainer-1").unwrap();

        assert_eq!(subject, "user:admin:maintainer-1");
        assert_ne!(subject, "maintainer-1");
        assert_eq!(permission.resource(), "product");
        assert_eq!(permission.action(), "update");
        assert!(!permission.covers(&Permission::parse("product:delete").unwrap()));
        assert!(!permission.covers(&Permission::parse("supplier_offering:update").unwrap()));
    }

    /// 不同维护人保留各自的主体，禁止交接目标共用授权身份。
    #[test]
    fn maintainer_authorization_keeps_distinct_account_subjects() {
        let first = maintainer_authorization(AccountKind::Admin, "maintainer-1").unwrap();
        let second = maintainer_authorization(AccountKind::Admin, "maintainer-2").unwrap();

        assert_ne!(first.0, second.0);
        assert_eq!(second.0, "user:admin:maintainer-2");
        assert_eq!(first.1, second.1);
    }
}
