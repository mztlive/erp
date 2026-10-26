//! 来源关键文案变化的人工核对；不提供公司 SKU 或单位身份变更捷径。
use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_catalog::entity::catalog::{EnableStatus, Sku, UnitOfMeasure};
use erp_catalog::repository::CatalogExt;
use erp_identity::SharedRbacService;
use erp_supply::dto::supplier_catalog::{CatalogIdentityReviewView, ReviewCatalogIdentityRequest};
use erp_supply::entity::supplier_catalog::{CatalogIdentityReviewReceipt, SupplierCatalogItem};
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::context::digest;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::SupplierApiGovernanceProcess;
use crate::adapters::{catalog_access, offering_access};
use crate::audit::persist_log;
use crate::supplier_connection_execution::apply_source_supply;
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
        self.ensure_permission(actor, "supplier_api_connection:detail").await?;
        let rbac = self.rbac.clone().ok_or_else(|| Error::Forbidden("未装配目录核对权限".into()))?;
        Ok(load_review(&self.db, rbac, connection, item, actor, false, &mut NoTransaction).await?.1)
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
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        self.ensure_permission(actor, "product:update").await?;
        self.ensure_permission(actor, "supplier_offering:update").await?;
        let rbac = self.rbac.clone().ok_or_else(|| Error::Forbidden("未装配目录核对权限".into()))?;
        let (db, connection, item, actor) =
            (self.db.clone(), connection.to_owned(), item.to_owned(), actor.clone());
        let bytes =
            serde_json::to_vec(&request).map_err(|_| Error::ValidationError("核对请求无法编码".into()))?;
        let fingerprint = hex::encode(Sha256::digest(bytes));
        let id =
            format!("catalog-review-{}", digest(&[&connection, &item, actor.id(), &request.idempotency_key]));
        let command = IdentityReviewCommand { connection, item, request, actor, id, fingerprint };
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { confirm_review(&db, rbac, command, executor).await })
            })
            .await
    }
}

async fn load_review(
    db: &Database,
    rbac: SharedRbacService,
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
    let offering = offering_access(db.clone(), rbac.clone())
        .require_offering(actor, if write { "update" } else { "list" }, offering_id, executor)
        .await?;
    let sku = db
        .skus()
        .find_by_id(offering.sku_id.as_ref(), executor)
        .await?
        .filter(|sku| item.sku_id.as_deref() == Some(sku.base.id.as_str()) && sku.is_active())
        .ok_or_else(|| Error::ConflictError("当前公司SKU关联不一致或已停用".into()))?;
    let product = catalog_access(db.clone(), rbac)
        .require_product(actor, if write { "update" } else { "detail" }, sku.product_id.as_ref(), executor)
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
    let view = review_view(&item, &sku, &unit, offering.base.version, current.specification.as_deref())?;
    Ok((item, view))
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
    rbac: SharedRbacService,
    command: IdentityReviewCommand,
    executor: &mut dyn Executor,
) -> Result<CatalogIdentityReviewView> {
    let IdentityReviewCommand { connection, item, request, actor, id, fingerprint } = command;

    let (mut current, mut result) = load_review(db, rbac, &connection, &item, &actor, true, executor).await?;
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
