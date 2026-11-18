//! 拥有带审计外层事务的仓库流程。

use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_catalog::CatalogExt;
use erp_core::ids::WarehouseSkuPolicyId;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::warehouse;
use erp_warehouse::entity::warehouse::status::EnableStatus;
use erp_warehouse::entity::warehouse::warehouse_sku_policy::{WarehouseSkuPolicy, WarehouseSkuPolicyData};
use erp_warehouse::{CreateWarehouseSkuPolicyRequest, WarehouseExt, WarehouseSkuPolicyView};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use crate::{Error, Result};

/// 创建仓库 SKU 策略，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 仓库、SKU 与策略所在数据库。
/// * `authorization` - 应用共享授权服务。
/// * `req` - 仓库 SKU 策略创建请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的策略视图。
///
/// # 错误
/// 请求校验失败、仓库或 SKU 不存在、策略构造失败、有效期重叠，或写入与审计事务失败时返回错误。
pub async fn create_warehouse_sku_policy(
    db: Database,
    authorization: Arc<AuthorizationService>,
    req: CreateWarehouseSkuPolicyRequest,
    actor: AuditActor,
) -> Result<WarehouseSkuPolicyView> {
    req.validate()?;

    let id = WarehouseSkuPolicyId::new(next_id());
    let policy = WarehouseSkuPolicy::new(
        id.clone(),
        WarehouseSkuPolicyData {
            warehouse_id: req.warehouse_id,
            sku_id: req.sku_id,
            minimum_available_quantity: req.minimum_available_quantity,
            status: req.status.unwrap_or(EnableStatus::Active),
            effective_from: req.effective_from,
            effective_to: req.effective_to,
        },
    )?;

    let audit =
        actor.clone().resource_log("warehouse_sku_policy.create", "warehouse_sku_policy", id.to_string())?;
    let policy_for_tx = policy.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move { persist_policy(db, &authorization, &actor, &policy_for_tx, executor).await })
    })
    .await?;
    Ok(policy.into())
}

/// 创建资格、维度存在性和区间重叠检查全部使用原写入事务。
async fn persist_policy(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    policy: &WarehouseSkuPolicy,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization.require(actor, &warehouse::SKU_POLICY_CREATE, AccessTarget::Operation, executor).await?;

    db.warehouse()
        .warehouse(policy.warehouse_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
    db.skus()
        .find_by_id(policy.sku_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("SKU不存在".to_string()))?;

    let existing =
        db.warehouse().sku_policies_for_dimensions(&policy.warehouse_id, &policy.sku_id, executor).await?;
    policy.ensure_no_overlap(&existing).map_err(|error| Error::BusinessLogicError(error.to_string()))?;

    db.warehouse_sku_policies().create(policy, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{SkuId, WarehouseId};
    use erp_core::money::Quantity;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn policy_creation_rejects_authorization_failure_before_dimension_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("policy_guard");
        let actor = AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin);
        let policy = WarehouseSkuPolicy::new(
            WarehouseSkuPolicyId::new("policy"),
            WarehouseSkuPolicyData {
                warehouse_id: WarehouseId::new("warehouse"),
                sku_id: SkuId::new("sku"),
                minimum_available_quantity: Quantity::from_str("1").unwrap(),
                status: EnableStatus::Active,
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap();

        let error = persist_policy(
            &db,
            &unavailable_authorization(),
            &actor,
            &policy,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
