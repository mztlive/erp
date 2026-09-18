//! 商品维护人交接：目标资格、组织启用、幂等收据。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_catalog::{HandoverCandidateView, HandoverProductRequest, HandoverProductView};
use erp_identity::{AccessControlExt, Permission, SharedRbacService};
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use crate::adapters::scoped_catalog_service;
use crate::handover_common::{ensure_org_enabled, ensure_replay_fingerprint, trimmed_idempotency_key};
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
    let audit_id = format!("product-handover-{}-{}-{key}", actor.id(), id);
    let fingerprint = handover_fingerprint(actor.id(), id, &req, &key)?;
    if let Some(existing) = replay(&db, &rbac, &audit_id, &fingerprint, id, actor).await? {
        return Ok(existing);
    }
    ensure_target_qualified(&db, &rbac, req.target_user_id.trim(), &mut NoTransaction).await?;
    ensure_org_enabled(&db, req.target_org_unit_id.as_deref(), &mut NoTransaction).await?;
    let db_tx = db.clone();
    let rbac_tx = rbac.clone();
    let actor_tx = actor.clone();
    let req_tx = req.clone();
    let id_tx = id.to_string();
    let audit_id_tx = audit_id.clone();
    let fingerprint_tx = fingerprint.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                ensure_target_qualified(&db_tx, &rbac_tx, req_tx.target_user_id.trim(), executor).await?;
                ensure_org_enabled(&db_tx, req_tx.target_org_unit_id.as_deref(), executor).await?;
                let view = scoped_catalog_service(db_tx.clone(), rbac_tx.clone())
                    .apply_product_handover(&id_tx, &req_tx, &actor_tx, executor)
                    .await?;
                let audit = actor_tx.clone().resource_log_with_id(
                    audit_id_tx,
                    "product.handover",
                    "product",
                    view.product_id.clone(),
                    Some(format!(
                        "command_sha256={fingerprint_tx};target={};reason={}",
                        req_tx.target_user_id.trim(),
                        req_tx.reason.trim()
                    )),
                )?;
                db_tx.audit_logs().create(&audit, executor).await?;
                Ok(view)
            })
        })
        .await
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
        if !account_can_maintain(&rbac, &account.base.id).await? {
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

/// 同一幂等键且载荷一致时回放已提交交接。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - RBAC 快照
/// * `audit_id` - 交接审计主键
/// * `expected_fingerprint` - 本次命令指纹
/// * `product_id` - 商品 ID
/// * `actor` - 已认证操作人
///
/// # 返回
/// 已存在且载荷一致时返回当前责任视图。
///
/// # 错误
/// 同一幂等键用于不同载荷时拒绝。
async fn replay(
    db: &Database,
    rbac: &SharedRbacService,
    audit_id: &str,
    expected_fingerprint: &str,
    product_id: &str,
    actor: &AuditActor,
) -> Result<Option<HandoverProductView>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
        return Ok(None);
    };
    ensure_replay_fingerprint(
        audit.message.as_deref(),
        expected_fingerprint,
        "同一幂等键已用于不同的商品交接",
    )?;
    let product = crate::adapters::catalog_access(db.clone(), rbac.clone())
        .require_product(actor, "update", product_id, &mut NoTransaction)
        .await?;
    Ok(Some(HandoverProductView {
        product_id: product.base.id,
        maintainer_user_id: product.maintainer_user_id,
        business_org_unit_id: product.business_org_unit_id,
        version: product.base.version,
    }))
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
    if !account.is_active_backoffice() || !account_can_maintain(rbac, target).await? {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备商品维护资格".into()));
    }
    Ok(())
}

/// 判断账号是否具备 `product:update`。
///
/// # 参数
/// * `rbac` - RBAC 快照
/// * `user_id` - 账号
///
/// # 返回
/// 具备维护资格时为 true。
///
/// # 错误
/// RBAC 判定失败时拒绝。
async fn account_can_maintain(rbac: &SharedRbacService, user_id: &str) -> Result<bool> {
    let permission = Permission::parse("product:update").map_err(Error::from)?;
    Ok(rbac.enforce(user_id, &permission).await?)
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
