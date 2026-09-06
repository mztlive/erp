use entities::procurement_responsibility::{
    collect_rule_list_ids, ProcurementResponsibilityRule, ProcurementRuleListDisplayFacts,
    ProcurementRuleListPage,
};
use erp_core::ids::SkuRevisionId;

use super::ids::unique_ids;
use super::ProcurementResponsibilityRuleFilter;
use crate::{CatalogExt, ProcurementResponsibilityExt};
use erp_identity::AccessControlExt;
use persistence_core::Executor;
use persistence_core::Result;

/// 批量加载规则行展示所需的最小关联事实.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `rules` - 当前页规则实体切片
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回负责人姓名、SKU 编号和当前名称、分类名称的稀疏映射；缺失引用保持稀疏.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除已通过 Repository 查询过滤.
///
/// # 约束
/// 查询次数固定为 4 次（负责人、SKU、SKU 当前修订、分类各一次），与页大小无关；空输入直接返回空映射.
pub async fn load_procurement_rule_list_facts(
    db: &mongodb::Database,
    rules: &[ProcurementResponsibilityRule],
    executor: &mut dyn Executor,
) -> Result<ProcurementRuleListDisplayFacts> {
    use std::collections::HashMap;

    if rules.is_empty() {
        return Ok(ProcurementRuleListDisplayFacts::default());
    }
    let (owner_ids, sku_ids, category_ids) = collect_rule_list_ids(rules);
    let mut facts = ProcurementRuleListDisplayFacts::default();
    if !owner_ids.is_empty() {
        let owners = db
            .accounts()
            .list_procurement_responsibility_owners(&owner_ids, executor)
            .await?;
        facts.owner_names = owners
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect::<HashMap<_, _>>();
    }
    if !sku_ids.is_empty() {
        let skus = db
            .skus()
            .list_procurement_responsibility_skus(&sku_ids, executor)
            .await?;
        let revision_ids: Vec<SkuRevisionId> = unique_ids(
            skus.iter()
                .filter_map(|sku| sku.stable.current_revision_id.as_deref().map(SkuRevisionId::new)),
        );
        let revision_names = if revision_ids.is_empty() {
            HashMap::new()
        } else {
            db.sku_revisions()
                .list_procurement_responsibility_sku_revisions(&revision_ids, executor)
                .await?
                .into_iter()
                .map(|revision| (revision.base.id, revision.name))
                .collect::<HashMap<_, _>>()
        };
        for sku in skus {
            facts.sku_nos.insert(sku.base.id.clone(), sku.sku_no.clone());
            if let Some(revision_id) = sku.stable.current_revision_id.as_deref() {
                if let Some(name) = revision_names.get(revision_id) {
                    facts.sku_names.insert(sku.base.id, name.clone());
                }
            }
        }
    }
    if !category_ids.is_empty() {
        let categories = db
            .product_categories()
            .list_procurement_responsibility_categories(&category_ids, executor)
            .await?;
        facts.category_names = categories
            .into_iter()
            .map(|category| (category.base.id, category.name))
            .collect::<HashMap<_, _>>();
    }
    Ok(facts)
}

/// 分页查询规则行并批量返回管理列表展示事实.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `filter` - 规则类型、负责人、状态及分页筛选
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回按优先级与创建时间稳定排序的当前页规则、总数及展示事实.
///
/// # 错误
/// MongoDB 查询、计数或反序列化失败时返回错误；总数、排序与软删除语义与规则集合查询一致.
///
/// # 约束
/// 分页先按过滤求总数再取当前页，关联查询放在分页之后但次数固定为 4 次；不得读取分页外规则.
pub async fn load_procurement_rule_list_page(
    db: &mongodb::Database,
    filter: &ProcurementResponsibilityRuleFilter,
    executor: &mut dyn Executor,
) -> Result<ProcurementRuleListPage> {
    let page = db
        .procurement_responsibility_rules()
        .search_procurement_responsibility_rules(filter, executor)
        .await?;
    let facts = load_procurement_rule_list_facts(db, &page.items, executor).await?;
    Ok(ProcurementRuleListPage {
        items: page.items,
        total: page.total,
        facts,
    })
}
