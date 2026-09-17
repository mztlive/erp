//! 供给列表采购负责人筛选：规则解析，不落主档。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_catalog::entity::catalog::{Product, ProductKind, ProductRevision};
use erp_core::ids::{ProductCategoryId, SkuId, SupplierOfferingId};
use erp_procurement::entity::facts::ProductKind as ProcurementProductKind;
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityContext, ProcurementResponsibilityRuleSet,
};
use erp_procurement::repository::ProcurementResponsibilityExt;
use erp_supply::SupplierOfferingExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

const AUTHORIZED_ID_LIMIT: usize = 10_000;

/// 在授权供给集合内按采购规则解析负责人。
#[async_trait]
pub trait OfferingProcurementOwners: Send + Sync {
    /// 返回采购负责人落在请求集合内的供给 ID。
    ///
    /// # 参数
    /// * `authorized_ids` - 已授权供给；`None` 表示公司范围需先列举
    /// * `owner_user_ids` - 请求的采购负责人
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回命中的供给 ID。
    ///
    /// # 错误
    /// 授权集合超限时整体拒绝；缺规则的供给不命中。
    async fn matching_offering_ids(
        &self,
        authorized_ids: Option<&[String]>,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 生产实现：只读加载现行采购责任规则并按 SKU 批量 resolve。
pub struct MongoOfferingProcurementOwners {
    db: Database,
}

impl MongoOfferingProcurementOwners {
    /// 绑定采购规则与供给集合。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的解析器。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为共享 Port。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回供给列表可注入的采购负责人解析器。
    pub fn shared(db: Database) -> Arc<dyn OfferingProcurementOwners> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl OfferingProcurementOwners for MongoOfferingProcurementOwners {
    async fn matching_offering_ids(
        &self,
        authorized_ids: Option<&[String]>,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = owner_user_ids.iter().map(String::as_str).collect();
        let offering_ids = match authorized_ids {
            Some(ids) => ids.to_vec(),
            None => self.db.supplier_offerings().list_ids(executor).await?,
        };
        ensure_authorized_count(offering_ids.len())?;
        if offering_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rules = self
            .db
            .procurement_responsibility_rules()
            .list_active_procurement_responsibility_rules(executor)
            .await?;
        let offerings = load_offerings(&self.db, &offering_ids, executor).await?;
        let sku_ids: Vec<SkuId> = offerings.iter().map(|row| row.sku_id.clone()).collect();
        let skus = load_skus(&self.db, &sku_ids, executor).await?;
        let products = load_products(&self.db, &skus, executor).await?;
        let revisions = load_revisions(&self.db, &products, executor).await?;
        match_authorized_offerings(
            &self.db,
            OfferingMatchInput {
                offerings: &offerings,
                skus: &skus,
                products: &products,
                revisions: &revisions,
                rules: &rules,
                wanted: &wanted,
            },
            executor,
        )
        .await
    }
}

/// 内存桩：测试维护人与采购负责人分列。
#[derive(Default)]
pub struct MapOfferingProcurementOwners {
    /// 供给 ID 到采购负责人。
    pub owners: HashMap<String, String>,
}

#[async_trait]
impl OfferingProcurementOwners for MapOfferingProcurementOwners {
    async fn matching_offering_ids(
        &self,
        authorized_ids: Option<&[String]>,
        owner_user_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = owner_user_ids.iter().map(String::as_str).collect();
        Ok(self
            .owners
            .iter()
            .filter(|(offering_id, owner)| {
                wanted.contains(owner.as_str())
                    && authorized_ids.is_none_or(|ids| ids.iter().any(|id| id == *offering_id))
            })
            .map(|(id, _)| id.clone())
            .collect())
    }
}

struct OfferingMatchInput<'a> {
    offerings: &'a [erp_supply::entity::supplier_offering::SupplierOffering],
    skus: &'a [erp_catalog::entity::catalog::Sku],
    products: &'a HashMap<String, Product>,
    revisions: &'a HashMap<String, ProductRevision>,
    rules: &'a [erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRule],
    wanted: &'a HashSet<&'a str>,
}

async fn match_authorized_offerings(
    db: &Database,
    input: OfferingMatchInput<'_>,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let rule_set = ProcurementResponsibilityRuleSet::new(input.rules);
    let sku_by_id: HashMap<&str, &erp_catalog::entity::catalog::Sku> =
        input.skus.iter().map(|sku| (sku.base.id.as_str(), sku)).collect();
    let mut matched = HashSet::new();
    for offering in input.offerings {
        let Some(sku) = sku_by_id.get(offering.sku_id.as_ref()) else {
            continue;
        };
        if match_sku_owner(db, input.products, input.revisions, &rule_set, sku, input.wanted, executor)
            .await?
            .is_some()
        {
            matched.insert(offering.base.id.clone());
        }
    }
    Ok(matched.into_iter().collect())
}

async fn match_sku_owner(
    db: &Database,
    products: &HashMap<String, Product>,
    revisions: &HashMap<String, ProductRevision>,
    rule_set: &ProcurementResponsibilityRuleSet<'_>,
    sku: &erp_catalog::entity::catalog::Sku,
    wanted: &HashSet<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(product) = products.get(sku.product_id.as_ref()) else {
        return Ok(None);
    };
    let Some(revision_id) = product.stable.current_revision_id.as_deref() else {
        return Ok(None);
    };
    let Some(revision) = revisions.get(revision_id) else {
        return Ok(None);
    };
    let Ok(chain) = category_ids(db, &revision.category_id, executor).await else {
        return Ok(None);
    };
    let Ok(kind) = map_kind(product.product_kind) else {
        return Ok(None);
    };
    let Ok(context) =
        ProcurementResponsibilityContext::new(SkuId::new(sku.base.id.clone()), chain, None, kind)
    else {
        return Ok(None);
    };
    let Ok(rule) = rule_set.resolve(&context) else {
        return Ok(None);
    };
    Ok(wanted.contains(rule.owner_user_id.as_str()).then(|| product.base.id.clone()))
}

async fn load_offerings(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<erp_supply::entity::supplier_offering::SupplierOffering>> {
    let keys: Vec<SupplierOfferingId> = ids.iter().cloned().map(SupplierOfferingId::new).collect();
    db.supplier_offerings().list_by_ids(&keys, executor).await.map_err(Into::into)
}

async fn load_skus(
    db: &Database,
    sku_ids: &[SkuId],
    executor: &mut dyn Executor,
) -> Result<Vec<erp_catalog::entity::catalog::Sku>> {
    if sku_ids.is_empty() {
        return Ok(Vec::new());
    }
    db.skus().find_by_ids(sku_ids, executor).await.map_err(Into::into)
}

async fn load_products(
    db: &Database,
    skus: &[erp_catalog::entity::catalog::Sku],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Product>> {
    let ids: Vec<erp_core::ids::ProductId> =
        skus.iter().map(|sku| sku.product_id.clone()).collect::<HashSet<_>>().into_iter().collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let products = db.products().find_by_ids(&ids, executor).await?;
    Ok(products.into_iter().map(|product| (product.base.id.clone(), product)).collect())
}

async fn load_revisions(
    db: &Database,
    products: &HashMap<String, Product>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, ProductRevision>> {
    let ids: Vec<erp_core::ids::ProductRevisionId> = products
        .values()
        .filter_map(|product| product.stable.current_revision_id.clone())
        .map(erp_core::ids::ProductRevisionId::new)
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let revisions = db.product_revisions().find_by_ids(&ids, executor).await?;
    Ok(revisions.into_iter().map(|revision| (revision.base.id.clone(), revision)).collect())
}

async fn category_ids(
    db: &Database,
    category_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<ProductCategoryId>> {
    let mut chain = Vec::new();
    let mut current = Some(category_id.to_string());
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id.clone()) || chain.len() >= 32 {
            return Err(Error::ValidationError("商品分类链非法".into()));
        }
        chain.push(ProductCategoryId::new(id.clone()));
        current = db
            .product_categories()
            .find_by_id(&id, executor)
            .await?
            .and_then(|category| category.parent_category_id.map(|parent| parent.to_string()));
    }
    if chain.is_empty() {
        return Err(Error::ValidationError("商品分类链为空".into()));
    }
    Ok(chain)
}

fn map_kind(kind: ProductKind) -> Result<ProcurementProductKind> {
    match kind {
        ProductKind::Physical => Ok(ProcurementProductKind::Physical),
        ProductKind::Virtual => Ok(ProcurementProductKind::Virtual),
        ProductKind::OfflineService => Ok(ProcurementProductKind::OfflineService),
        ProductKind::Voucher => Ok(ProcurementProductKind::Voucher),
    }
}

/// 授权集合超限时整体拒绝，不得截断后继续解析。
///
/// # 参数
/// * `count` - 已授权供给数量
///
/// # 返回
/// 未超限时成功。
///
/// # 错误
/// 超过 10000 时返回校验错误。
pub(super) fn ensure_authorized_count(count: usize) -> Result<()> {
    if count > AUTHORIZED_ID_LIMIT {
        return Err(Error::ValidationError("采购负责人筛选超过上限，请收窄组织或维护人条件".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procurement_owner_filter_rejects_over_limit_authorized_set() {
        assert!(ensure_authorized_count(AUTHORIZED_ID_LIMIT).is_ok());
        let err = ensure_authorized_count(AUTHORIZED_ID_LIMIT + 1).unwrap_err();
        match err {
            Error::ValidationError(message) => assert!(message.contains("超过上限")),
            other => panic!("expected over-limit rejection, got {other:?}"),
        }
    }
}
