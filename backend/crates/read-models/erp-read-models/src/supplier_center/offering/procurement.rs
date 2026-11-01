//! 供给列表采购负责人筛选：规则解析，不落主档。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_catalog::repository::catalog::procurement_facts::{
    ProcurementProductFact, ProcurementRevisionFact, ProcurementSkuFact,
};
use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRuleSet;
use erp_procurement::repository::ProcurementResponsibilityExt;
use erp_procurement::repository::prelude::*;
use erp_supply::SupplierOfferingExt;
use erp_supply::repository::prelude::*;
use erp_supply::repository::supplier_offering::procurement::{
    ProcurementOfferingFact, ProcurementOfferingRepositoryExt,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::catalog_center::{CategoryChainCache, matches_procurement_owner};
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
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 错误
    /// 不返回错误。
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
        let sku_ids: Vec<String> = offerings.iter().map(|row| row.sku_id.clone()).collect();
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
    offerings: &'a [ProcurementOfferingFact],
    skus: &'a [ProcurementSkuFact],
    products: &'a HashMap<String, ProcurementProductFact>,
    revisions: &'a HashMap<String, ProcurementRevisionFact>,
    rules: &'a [erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRule],
    wanted: &'a HashSet<&'a str>,
}

/// 在授权供给内逐 SKU 解析负责人，并复用本次读取成功的分类事实。
async fn match_authorized_offerings(
    db: &Database,
    input: OfferingMatchInput<'_>,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let rule_set = ProcurementResponsibilityRuleSet::new(input.rules);
    let mut categories = CategoryChainCache::new(db);
    // Warm-up 仅缓存成功读取；失败时继续沿原逐 SKU 读取与跳过策略。
    let _ = categories.prefetch(input.revisions.values().map(|row| row.category_id.as_str()), executor).await;
    let sku_by_id: HashMap<&str, &ProcurementSkuFact> =
        input.skus.iter().map(|sku| (sku.id.as_str(), sku)).collect();
    let mut matched = HashSet::new();
    for offering in input.offerings {
        let Some(sku) = sku_by_id.get(offering.sku_id.as_str()) else {
            continue;
        };
        if match_sku_owner(
            &mut categories,
            input.products,
            input.revisions,
            &rule_set,
            sku,
            input.wanted,
            executor,
        )
        .await?
        .is_some()
        {
            matched.insert(offering.id.clone());
        }
    }
    Ok(matched.into_iter().collect())
}

/// 判定单个 SKU；规则缺失或分类读取失败时保持跳过该 SKU 的策略。
async fn match_sku_owner(
    categories: &mut CategoryChainCache<'_>,
    products: &HashMap<String, ProcurementProductFact>,
    revisions: &HashMap<String, ProcurementRevisionFact>,
    rule_set: &ProcurementResponsibilityRuleSet<'_>,
    sku: &ProcurementSkuFact,
    wanted: &HashSet<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(product) = products.get(sku.product_id.as_str()) else {
        return Ok(None);
    };
    let Some(revision_id) = product.current_revision_id.as_deref() else {
        return Ok(None);
    };
    let Some(revision) = revisions.get(revision_id) else {
        return Ok(None);
    };
    let Ok(chain) = categories.ids(&revision.category_id, executor).await else {
        return Ok(None);
    };
    Ok(matches_procurement_owner(product.product_kind, &sku.id, chain, rule_set, wanted)
        .then(|| product.id.clone()))
}

/// 只加载候选供给的稳定身份与 SKU 引用。
async fn load_offerings(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<ProcurementOfferingFact>> {
    db.supplier_offerings().procurement_facts(ids, executor).await.map_err(Into::into)
}

/// 批量读取供给所引 SKU 的稳定身份与所属商品。
async fn load_skus(
    db: &Database,
    sku_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<ProcurementSkuFact>> {
    db.catalog().procurement_skus(sku_ids, executor).await.map_err(Into::into)
}

/// 商品去重后读取类型和当前修订指针。
async fn load_products(
    db: &Database,
    skus: &[ProcurementSkuFact],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, ProcurementProductFact>> {
    let ids =
        skus.iter().map(|sku| sku.product_id.clone()).collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
    let products = db.catalog().procurement_products(&ids, executor).await?;
    Ok(products.into_iter().map(|product| (product.id.clone(), product)).collect())
}

/// 当前修订只读取分类引用，保留稳定商品当前指针的唯一来源。
async fn load_revisions(
    db: &Database,
    products: &HashMap<String, ProcurementProductFact>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, ProcurementRevisionFact>> {
    let ids = products.values().filter_map(|product| product.current_revision_id.clone()).collect::<Vec<_>>();
    let revisions = db.catalog().procurement_revisions(&ids, executor).await?;
    Ok(revisions.into_iter().map(|revision| (revision.id.clone(), revision)).collect())
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
