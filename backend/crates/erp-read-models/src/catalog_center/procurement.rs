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

mod category_chain;

pub(crate) use category_chain::CategoryChainCache;

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
    let mut categories = CategoryChainCache::new(db);
    let mut matched = HashSet::new();
    for sku in skus {
        if let Some(id) =
            match_sku_owner(&mut categories, products, revisions, &rule_set, sku, wanted, executor).await?
        {
            matched.insert(id);
        }
    }
    Ok(matched.into_iter().collect())
}

/// 解析单个 SKU 的采购负责人；规则缺失时跳过。
///
/// # 参数
/// * `categories` - 当前请求内的成功分类读取缓存
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
/// 无；规则缺失、分类链非法或读取失败时不命中，缺失的分类末节点继续保留。
async fn match_sku_owner(
    categories: &mut CategoryChainCache<'_>,
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
    let Ok(chain) = categories.ids(&revision.category_id, executor).await else {
        return Ok(None);
    };
    Ok(matches_procurement_owner(product.product_kind, &sku.base.id, chain, rule_set, wanted)
        .then(|| product.base.id.clone()))
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

/// 以现行规则判定单个 SKU 的负责人是否命中，分类顺序由读取链保持。
///
/// # 参数
/// * `kind` - 商品类型
/// * `sku_id` - SKU 身份
/// * `chain` - 从当前分类到根的链，含读取时缺失的末节点
/// * `rule_set` - 本次请求读取的采购责任规则
/// * `wanted` - 请求的负责人集合
///
/// # 返回
/// 返回负责人是否命中；无规则或上下文非法时返回 `false`。
///
/// # 错误
/// 无；不改变列表对非法关联的跳过策略。
pub(crate) fn matches_procurement_owner(
    kind: ProductKind,
    sku_id: &str,
    chain: Vec<ProductCategoryId>,
    rule_set: &ProcurementResponsibilityRuleSet<'_>,
    wanted: &HashSet<&str>,
) -> bool {
    let Ok(context) = ProcurementResponsibilityContext::new(SkuId::new(sku_id), chain, None, map_kind(kind))
    else {
        return false;
    };
    rule_set.resolve(&context).is_ok_and(|rule| wanted.contains(rule.owner_user_id.as_str()))
}

/// 穷尽映射商品类型，保留采购责任规则原来的类型选择器。
fn map_kind(kind: ProductKind) -> ProcurementProductKind {
    match kind {
        ProductKind::Physical => ProcurementProductKind::Physical,
        ProductKind::Virtual => ProcurementProductKind::Virtual,
        ProductKind::OfflineService => ProcurementProductKind::OfflineService,
        ProductKind::Voucher => ProcurementProductKind::Voucher,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::ProcurementResponsibilityRuleId;
    use erp_procurement::entity::procurement_responsibility::{
        EnableStatus, ProcurementResponsibilityRule, ProcurementResponsibilityRuleData,
        ProcurementResponsibilityRuleType,
    };

    use super::*;

    /// 创建用于实际负责人匹配的现行规则。
    fn rule(
        id: &str,
        rule_type: ProcurementResponsibilityRuleType,
        sku: Option<&str>,
        category: Option<&str>,
        kind: Option<ProcurementProductKind>,
        owner: &str,
    ) -> ProcurementResponsibilityRule {
        ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new(id),
            ProcurementResponsibilityRuleData {
                rule_type,
                sku_id: sku.map(SkuId::new),
                category_id: category.map(ProductCategoryId::new),
                service_region: None,
                product_kind: kind,
                owner_user_id: owner.to_string(),
                status: EnableStatus::Active,
            },
            "admin",
        )
        .unwrap()
    }

    /// 构造当前分类与祖先分类顺序。
    fn chain() -> Vec<ProductCategoryId> {
        ["leaf", "parent"].map(ProductCategoryId::new).to_vec()
    }

    #[test]
    /// 商品类型均保持对应的采购类型规则，防止跨类型命中。
    fn procurement_owner_matching_preserves_product_kind_mapping() {
        for (kind, procurement_kind) in [
            (ProductKind::Physical, ProcurementProductKind::Physical),
            (ProductKind::Virtual, ProcurementProductKind::Virtual),
            (ProductKind::OfflineService, ProcurementProductKind::OfflineService),
            (ProductKind::Voucher, ProcurementProductKind::Voucher),
        ] {
            let rules = [rule(
                "kind",
                ProcurementResponsibilityRuleType::ProductKind,
                None,
                None,
                Some(procurement_kind),
                "owner",
            )];
            let rule_set = ProcurementResponsibilityRuleSet::new(&rules);
            assert!(matches_procurement_owner(kind, "sku", chain(), &rule_set, &HashSet::from(["owner"])));
            assert!(!matches_procurement_owner(kind, "sku", chain(), &rule_set, &HashSet::from(["other"])));
        }
    }

    #[test]
    /// 继续按 SKU、当前分类、父分类、类型、默认负责人优先级匹配。
    fn procurement_owner_matching_preserves_rule_priority_and_nearest_category() {
        let rules = [
            rule(
                "default",
                ProcurementResponsibilityRuleType::DefaultDispatcher,
                None,
                None,
                None,
                "default-owner",
            ),
            rule(
                "kind",
                ProcurementResponsibilityRuleType::ProductKind,
                None,
                None,
                Some(ProcurementProductKind::Physical),
                "kind-owner",
            ),
            rule(
                "parent",
                ProcurementResponsibilityRuleType::Category,
                None,
                Some("parent"),
                None,
                "parent-owner",
            ),
            rule("leaf", ProcurementResponsibilityRuleType::Category, None, Some("leaf"), None, "leaf-owner"),
            rule("sku", ProcurementResponsibilityRuleType::Sku, Some("sku"), None, None, "sku-owner"),
        ];
        for (length, expected_owner) in [
            (5, "sku-owner"),
            (4, "leaf-owner"),
            (3, "parent-owner"),
            (2, "kind-owner"),
            (1, "default-owner"),
        ] {
            let rule_set = ProcurementResponsibilityRuleSet::new(&rules[..length]);
            assert!(matches_procurement_owner(
                ProductKind::Physical,
                "sku",
                chain(),
                &rule_set,
                &HashSet::from([expected_owner])
            ));
        }
        let rule_set = ProcurementResponsibilityRuleSet::new(&rules[..4]);
        assert!(!matches_procurement_owner(
            ProductKind::Physical,
            "sku",
            chain(),
            &rule_set,
            &HashSet::from(["parent-owner"])
        ));
    }

    #[test]
    /// 缺失规则、同层歧义和非法上下文仍跳过，不扩大负责人匹配范围。
    fn procurement_owner_matching_skips_missing_ambiguous_or_invalid_rules() {
        let wanted = HashSet::from(["owner"]);
        let empty = ProcurementResponsibilityRuleSet::new(&[]);
        assert!(!matches_procurement_owner(ProductKind::Physical, "sku", chain(), &empty, &wanted));
        let rules = [
            rule("first", ProcurementResponsibilityRuleType::Category, None, Some("leaf"), None, "owner"),
            rule("second", ProcurementResponsibilityRuleType::Category, None, Some("leaf"), None, "owner"),
        ];
        let ambiguous = ProcurementResponsibilityRuleSet::new(&rules);
        assert!(!matches_procurement_owner(ProductKind::Physical, "sku", chain(), &ambiguous, &wanted));
        let single = ProcurementResponsibilityRuleSet::new(&rules[..1]);
        assert!(!matches_procurement_owner(ProductKind::Physical, "sku", Vec::new(), &single, &wanted));
        assert!(!matches_procurement_owner(
            ProductKind::Physical,
            "sku",
            ["leaf", "leaf"].map(ProductCategoryId::new).to_vec(),
            &single,
            &wanted
        ));
    }
}
