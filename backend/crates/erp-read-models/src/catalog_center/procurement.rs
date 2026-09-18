//! 商品列表采购负责人筛选：规则解析，不落主档。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_catalog::entity::catalog::{Product, ProductKind, ProductRevision};
use erp_catalog::repository::prelude::*;
use erp_core::ids::{ProductCategoryId, SkuId};
use erp_procurement::entity::facts::ProductKind as ProcurementProductKind;
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityContext, ProcurementResponsibilityRuleSet,
};
use erp_procurement::repository::ProcurementResponsibilityExt;
use erp_procurement::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

const AUTHORIZED_ID_LIMIT: usize = 10_000;

/// 在授权商品集合内按采购规则解析负责人。
#[async_trait]
pub trait ProductProcurementOwners: Send + Sync {
    /// 返回采购负责人落在请求集合内的商品 ID。
    ///
    /// # 参数
    /// * `authorized_ids` - 已授权商品；`None` 表示公司范围需先列举
    /// * `owner_user_ids` - 请求的采购负责人
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回命中的商品 ID。
    ///
    /// # 错误
    /// 授权集合超限时整体拒绝；缺规则的商品不命中。
    async fn matching_product_ids(
        &self,
        authorized_ids: Option<&[String]>,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 生产实现：只读加载现行采购责任规则并批量 resolve。
pub struct MongoProductProcurementOwners {
    db: Database,
}

impl MongoProductProcurementOwners {
    /// 绑定采购规则与商品集合。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的解析器。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为共享 Port。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回商品列表可注入的采购负责人解析器。
    ///
    /// # 错误
    /// 无。
    pub fn shared(db: Database) -> Arc<dyn ProductProcurementOwners> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl ProductProcurementOwners for MongoProductProcurementOwners {
    async fn matching_product_ids(
        &self,
        authorized_ids: Option<&[String]>,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = owner_user_ids.iter().map(String::as_str).collect();
        let product_ids = match authorized_ids {
            Some(ids) => ids.to_vec(),
            None => self.db.products().list_ids(executor).await?,
        };
        if product_ids.len() > AUTHORIZED_ID_LIMIT {
            return Err(Error::ValidationError("采购负责人筛选超过上限，请收窄组织或维护人条件".into()));
        }
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rules = self
            .db
            .procurement_responsibility_rules()
            .list_active_procurement_responsibility_rules(executor)
            .await?;
        let products = load_products(&self.db, &product_ids, executor).await?;
        let skus = load_skus(&self.db, &product_ids, executor).await?;
        let revisions = load_revisions(&self.db, &products, executor).await?;
        match_authorized_products(&self.db, &products, &skus, &revisions, &rules, &wanted, executor).await
    }
}

/// 内存桩：测试维护人与采购负责人分列。
#[derive(Default)]
pub struct MapProductProcurementOwners {
    /// 商品 ID 到采购负责人。
    pub owners: HashMap<String, String>,
}

#[async_trait]
impl ProductProcurementOwners for MapProductProcurementOwners {
    async fn matching_product_ids(
        &self,
        authorized_ids: Option<&[String]>,
        owner_user_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = owner_user_ids.iter().map(String::as_str).collect();
        Ok(self
            .owners
            .iter()
            .filter(|(product_id, owner)| {
                wanted.contains(owner.as_str())
                    && authorized_ids.is_none_or(|ids| ids.iter().any(|id| id == *product_id))
            })
            .map(|(id, _)| id.clone())
            .collect())
    }
}

/// 在授权商品集合内按现行规则解析采购负责人，缺规则的商品不命中。
///
/// # 参数
/// * `db` - 数据库
/// * `products` - 已加载商品
/// * `skus` - 商品下 SKU
/// * `revisions` - 当前商品修订
/// * `rules` - 现行采购责任规则
/// * `wanted` - 请求的采购负责人
/// * `executor` - 与授权相同的执行器
///
/// # 返回
/// 返回命中的商品 ID。
///
/// # 错误
/// 分类链读取失败时跳过该 SKU，不扩大授权。
async fn match_authorized_products(
    db: &Database,
    products: &HashMap<String, Product>,
    skus: &[erp_catalog::entity::catalog::Sku],
    revisions: &HashMap<String, ProductRevision>,
    rules: &[erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRule],
    wanted: &HashSet<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let rule_set = ProcurementResponsibilityRuleSet::new(rules);
    let mut matched = HashSet::new();
    for sku in skus {
        if let Some(id) = match_sku_owner(db, products, revisions, &rule_set, sku, wanted, executor).await? {
            matched.insert(id);
        }
    }
    Ok(matched.into_iter().collect())
}

/// 解析单个 SKU 的采购负责人；规则缺失时跳过。
///
/// # 参数
/// * `db` - 数据库
/// * `products` - 已加载商品
/// * `revisions` - 当前商品修订
/// * `rule_set` - 现行规则集合
/// * `sku` - 待解析 SKU
/// * `wanted` - 请求的采购负责人
/// * `executor` - 与授权相同的执行器
///
/// # 返回
/// 命中时返回商品 ID。
///
/// # 错误
/// 无；规则或分类缺失视为不命中。
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

async fn load_products(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Product>> {
    let products = db
        .products()
        .find_by_ids(
            &ids.iter().map(|id| erp_core::ids::ProductId::new(id.clone())).collect::<Vec<_>>(),
            executor,
        )
        .await?;
    Ok(products.into_iter().map(|product| (product.base.id.clone(), product)).collect())
}

async fn load_skus(
    db: &Database,
    product_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<erp_catalog::entity::catalog::Sku>> {
    db.skus()
        .find_by_product_ids(
            &product_ids.iter().map(|id| erp_core::ids::ProductId::new(id.clone())).collect::<Vec<_>>(),
            executor,
        )
        .await
        .map_err(Into::into)
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
