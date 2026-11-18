//! 来源关键文案变化的人工核对；不提供公司 SKU 或单位身份变更捷径。
use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_catalog::SkuRevision;
use erp_catalog::entity::catalog::{EnableStatus, Sku, UnitOfMeasure};
use erp_catalog::repository::CatalogExt;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{catalog as catalog_permissions, catalog, supplier_api, supplier_offering};
use erp_supply::dto::supplier_catalog::{CatalogIdentityReviewView, ReviewCatalogIdentityRequest};
use erp_supply::entity::supplier_catalog::{CatalogIdentityReviewReceipt, SupplierCatalogItem};
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::context::digest;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use sha2::{Digest, Sha256};

use super::SupplierApiGovernanceProcess;
use crate::supplier_connection_execution::apply_source_supply;
use crate::wiring::CatalogServices;
use crate::{Error, Result};

impl SupplierApiGovernanceProcess {
    /// 读取人工核对所需的当前来源、公司规格、单位与并发版本。
    /// # 参数
    /// 连接、目录身份及已认证操作人。
    /// # 返回
    /// 按公司商品和供给范围授权的核对快照，不含采购成本。
    /// # 错误
    /// 无权限、越范围、缺少已选品关联或单位时拒绝。
    pub async fn catalog_identity_review(
        &self,
        connection: &str,
        item: &str,
        actor: &AuditActor,
    ) -> Result<CatalogIdentityReviewView> {
        let services = self.services.clone();
        let authorization = self.authorization.clone();
        let (db, connection, item, actor) =
            (self.db.clone(), connection.to_owned(), item.to_owned(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorize_review_read(&authorization, &connection, &actor, executor).await?;
                    Ok(load_review(&db, &services, &connection, &item, &actor, false, executor).await?.1)
                })
            })
            .await
    }

    /// 确认来源文案仍对应当前实际销售项；实体身份变化须专门重新建档。
    /// # 参数
    /// 来源/公司/供给版本、原文、同一销售项确认、单位、原因和操作号。
    /// # 返回
    /// 同事务保存核对事实与回执；同号同载荷返回原结果。
    /// # 错误
    /// 越权、版本变化、单位不符、未明确核对、来源撤回或同号异载荷时拒绝。
    pub async fn review_catalog_identity(
        &self,
        connection: &str,
        item: &str,
        request: ReviewCatalogIdentityRequest,
        actor: &AuditActor,
    ) -> Result<CatalogIdentityReviewView> {
        self.ensure_permission(actor, &supplier_api::ADOPTIONS_PREVIEW).await?;
        self.ensure_permission(actor, &catalog::PRODUCT_PRODUCT_UPDATE).await?;
        self.ensure_permission(actor, &supplier_offering::REVISE).await?;

        let services = self.services.clone();
        let (db, connection, item, actor) =
            (self.db.clone(), connection.to_owned(), item.to_owned(), actor.clone());
        let bytes =
            serde_json::to_vec(&request).map_err(|_| Error::ValidationError("核对请求无法编码".into()))?;
        let fingerprint = hex::encode(Sha256::digest(bytes));
        let id =
            format!("catalog-review-{}", digest(&[&connection, &item, actor.id(), &request.idempotency_key]));
        let authorization = self.authorization.clone();
        let command = IdentityReviewCommand { connection, item, request, actor, id, fingerprint };

        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &command.actor,
                            &supplier_api::ADOPTIONS_PREVIEW,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    confirm_review(&db, &services, command, executor).await
                })
            })
            .await
    }
}

/// 连接、供给、商品和资料必须沿同一快照读取，不将对象授权带入非事务查询。
async fn authorize_review_read(
    authorization: &AuthorizationService,
    connection: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("商品身份核对读取必须使用快照事务".into()));
    }
    authorization
        .require(actor, &supplier_api::CONNECTION_DETAIL, AccessTarget::record(connection)?, executor)
        .await?;
    Ok(())
}

async fn load_review(
    db: &Database,
    services: &CatalogServices,
    connection: &str,
    item: &str,
    actor: &AuditActor,
    write: bool,
    executor: &mut dyn Executor,
) -> Result<(SupplierCatalogItem, CatalogIdentityReviewView)> {
    let item = db
        .supplier_catalog_items()
        .find_by_id(item, executor)
        .await?
        .filter(|item| item.connection_id == connection)
        .ok_or_else(|| Error::NotFound("供应商商品不存在".into()))?;
    let offering_id = item
        .offering_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("当前来源尚未完成选品".into()))?;

    let offering_permission = if write { supplier_offering::REVISE } else { supplier_offering::LIST };
    let offering =
        services.offering_access.require_offering(actor, &offering_permission, offering_id, executor).await?;
    let sku = db
        .skus()
        .find_by_id(offering.sku_id.as_ref(), executor)
        .await?
        .filter(|sku| item.sku_id.as_deref() == Some(sku.base.id.as_str()) && sku.is_active())
        .ok_or_else(|| Error::ConflictError("当前公司SKU关联不一致或已停用".into()))?;

    let product_permission = if write {
        catalog_permissions::PRODUCT_PRODUCT_UPDATE
    } else {
        catalog_permissions::PRODUCT_PRODUCT_DETAIL
    };
    let product = services
        .catalog_access
        .require_product(actor, &product_permission, sku.product_id.as_ref(), executor)
        .await?;
    if !product.is_active() {
        return Err(Error::BusinessLogicError("公司商品已停用，不能恢复自动使用".into()));
    }

    let unit = db
        .unit_of_measures()
        .find_by_id(sku.base_unit_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("公司商品单位不存在".into()))?;
    if unit.stable.status != EnableStatus::Active {
        return Err(Error::BusinessLogicError("公司商品单位已停用".into()));
    }
    item.ensure_company_unit(&[&unit.unit_code, &unit.name, &unit.symbol])?;

    let current = review_sku_content(db, &sku, &item, write, executor).await?;
    let view = review_view(&item, &sku, &unit, offering.base.version, current.specification.as_deref())?;
    Ok((item, view))
}

async fn review_sku_content(
    db: &Database,
    sku: &Sku,
    item: &SupplierCatalogItem,
    write: bool,
    executor: &mut dyn Executor,
) -> Result<SkuRevision> {
    let current_id = sku
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::ConflictError("公司SKU缺少当前资料".into()))?;
    let current = db
        .sku_revisions()
        .find_by_id(current_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("公司规格资料不存在".into()))?;

    if write
        && item
            .barcode
            .as_ref()
            .zip(current.barcode.as_ref())
            .is_some_and(|(source, company)| source != company)
    {
        return Err(Error::ConflictError(
            "来源条码与公司 SKU 条码不一致，请先核对商品条码，不能按文案变化恢复供给".into(),
        ));
    }
    Ok(current)
}

fn review_view(
    item: &SupplierCatalogItem,
    sku: &Sku,
    unit: &UnitOfMeasure,
    offering_version: u64,
    specification: Option<&str>,
) -> Result<CatalogIdentityReviewView> {
    Ok(CatalogIdentityReviewView {
        item_id: item.base.id.clone(),
        item_version: item.base.version,
        source_specification: item.specification.clone(),
        source_unit: item.unit.clone(),
        company_sku_id: sku.base.id.clone(),
        company_sku_no: sku.sku_no.clone(),
        company_specification: specification.unwrap_or("未填写规格说明，请核对公司商品资料").to_owned(),
        company_unit_id: sku.base_unit_id.to_string(),
        company_unit_code: unit.unit_code.clone(),
        company_unit_name: unit.name.clone(),
        sku_revision_id: sku
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::ConflictError("公司SKU缺少当前资料".into()))?,
        offering_version,
        requires_identity_review: item.requires_identity_review,
    })
}

struct IdentityReviewCommand {
    connection: String,
    item: String,
    request: ReviewCatalogIdentityRequest,
    actor: AuditActor,
    id: String,
    fingerprint: String,
}

async fn confirm_review(
    db: &Database,
    services: &CatalogServices,
    command: IdentityReviewCommand,
    executor: &mut dyn Executor,
) -> Result<CatalogIdentityReviewView> {
    let IdentityReviewCommand { connection, item, request, actor, id, fingerprint } = command;
    let (mut current, mut result) =
        load_review(db, services, &connection, &item, &actor, true, executor).await?;

    if let Some(receipt) = db.supplier_catalog_identity_reviews().find_by_id(&id, executor).await? {
        if receipt.fingerprint != fingerprint || receipt.actor_id != actor.id() || receipt.base.is_deleted() {
            return Err(Error::ConflictError("同一操作号已经用于其他来源核对".into()));
        }
        return Ok(receipt.result);
    }

    result.ensure_current(&request)?;
    current.confirm_identity_review(&request)?;
    db.supplier_catalog_items().update(&mut current, executor).await?;
    apply_source_supply(db, &current, &actor, executor).await?;

    result.requires_identity_review = false;
    result.item_version = current.base.version;
    let receipt = CatalogIdentityReviewReceipt {
        base: BaseModel::new(id),
        item_id: item,
        actor_id: actor.id().into(),
        fingerprint,
        reason: request.reason.trim().into(),
        result: result.clone(),
    };
    db.supplier_catalog_identity_reviews().create(&receipt, executor).await?;

    let audit = actor.resource_log_with_message(
        "supplier_catalog.identity.review",
        "supplier_api_connection",
        connection,
        Some(request.reason),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(result)
}

#[cfg(test)]
mod read_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 禁止非事务读取；同一快照内的授权失败必须先于来源及商品资料读取。
    #[tokio::test]
    async fn review_read_requires_snapshot_and_propagates_authorization_failure() {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let authorization = unavailable_authorization();
        let error = authorize_review_read(&authorization, "connection", &actor, &mut ReadExecutor::default())
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "商品身份核对读取必须使用快照事务")
        );

        let error =
            authorize_review_read(&authorization, "connection", &actor, &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
