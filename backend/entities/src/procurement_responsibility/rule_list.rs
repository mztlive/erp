//! 采购责任规则管理列表读模型.
//!
//! Repository 负责分页、总数、排序、软删除与批量关联事实；本模块只定义 I/O-free 的读模型与纯 ID 收集规则.

use std::collections::HashMap;

use crate::ids::{ProductCategoryId, SkuId};

use super::rule::ProcurementResponsibilityRule;

/// 规则管理列表展示所需的最小关联事实.
///
/// # 参数
/// * `owner_names` - 负责人账号 ID 到当前姓名的稀疏映射
/// * `sku_nos` - SKU ID 到业务编号的稀疏映射
/// * `sku_names` - SKU ID 到当前修订名称的稀疏映射
/// * `category_names` - 分类 ID 到当前名称的稀疏映射
///
/// # 返回
/// 返回分页内规则行展示所需的全部关联事实.
///
/// # 错误
/// 无；缺失历史引用时保持稀疏，由 Service 保留空展示语义.
///
/// # 约束
/// 全部映射均为稀疏返回，不做缺失校验；与分页顺序无关.
#[derive(Debug, Clone, Default)]
pub struct ProcurementRuleListDisplayFacts {
    /// 负责人账号 ID 到当前姓名的稀疏映射.
    pub owner_names: HashMap<String, String>,
    /// SKU ID 到业务编号的稀疏映射.
    pub sku_nos: HashMap<String, String>,
    /// SKU ID 到当前修订名称的稀疏映射.
    pub sku_names: HashMap<String, String>,
    /// 分类 ID 到当前名称的稀疏映射.
    pub category_names: HashMap<String, String>,
}

/// 规则管理分页读模型.
///
/// # 参数
/// * `items` - 当前页规则实体，按优先级与创建时间稳定排序
/// * `total` - 满足筛选条件的规则总数
/// * `facts` - 当前页展示所需的最小关联事实
///
/// # 返回
/// 返回规则行及负责人名、SKU 编号和当前名称、分类名所需事实.
///
/// # 错误
/// 无；分页总数、排序和软删除条件与规则集合查询完全一致.
///
/// # 约束
/// 关联查询次数与页大小无关；缺失历史引用时保持空展示语义.
#[derive(Debug, Clone)]
pub struct ProcurementRuleListPage {
    /// 当前页规则实体.
    pub items: Vec<ProcurementResponsibilityRule>,
    /// 满足筛选条件的规则总数.
    pub total: i64,
    /// 当前页展示所需的最小关联事实.
    pub facts: ProcurementRuleListDisplayFacts,
}

/// 从规则行收集去重后的关联 ID，供批量事实加载复用.
///
/// # 参数
/// * `rules` - 当前页规则实体切片
///
/// # 返回
/// 返回按字典序稳定排序的负责人 ID、SKU ID 与分类 ID 三元组.
///
/// # 错误
/// 无；纯内存计算，不访问数据库.
///
/// # 约束
/// 去重后按字符串字典序排序，保证批量 `$in` 顺序确定；与目录值对象共用同一排序实现.
pub fn collect_rule_list_ids(
    rules: &[ProcurementResponsibilityRule],
) -> (Vec<String>, Vec<SkuId>, Vec<ProductCategoryId>) {
    let owner_ids: Vec<String> =
        super::catalog::dedup_sorted_ids(rules.iter().map(|rule| rule.owner_user_id.clone()));
    let sku_ids: Vec<SkuId> =
        super::catalog::dedup_sorted_ids(rules.iter().filter_map(|rule| rule.sku_id.clone()));
    let category_ids: Vec<ProductCategoryId> =
        super::catalog::dedup_sorted_ids(rules.iter().filter_map(|rule| rule.category_id.clone()));
    (owner_ids, sku_ids, category_ids)
}

#[cfg(test)]
mod tests {
    use crate::catalog::EnableStatus;
    use crate::ids::{ProcurementResponsibilityRuleId, ProductCategoryId, SkuId};
    use crate::procurement_responsibility::{
        ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
    };

    use super::*;

    fn test_rule(
        id: &str,
        rule_type: ProcurementResponsibilityRuleType,
        sku_id: Option<&str>,
        category_id: Option<&str>,
        owner: &str,
    ) -> ProcurementResponsibilityRule {
        ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new(id),
            ProcurementResponsibilityRuleData {
                rule_type,
                sku_id: sku_id.map(SkuId::new),
                category_id: category_id.map(ProductCategoryId::new),
                service_region: None,
                product_kind: None,
                owner_user_id: owner.to_string(),
                status: EnableStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    /// 空规则行收集为空 ID 集合，事实加载可短路零查询.
    ///
    /// # 参数
    /// 无.
    ///
    /// # 返回
    /// 断言三类 ID 均为零长度.
    ///
    /// # 错误
    /// 收集逻辑回归时测试失败.
    ///
    /// # 约束
    /// Batch 空输入必须零查询；Page/Aggregation/Index 标记 N/A.
    #[test]
    fn empty_rules_collect_empty_ids() {
        let (owners, skus, categories) = collect_rule_list_ids(&[]);
        assert!(owners.is_empty());
        assert!(skus.is_empty());
        assert!(categories.is_empty());
    }

    /// 重复引用去重并按字典序稳定排序，批量 `$in` 与页大小无关.
    ///
    /// # 参数
    /// 无，内部构造重复负责人与选择器.
    ///
    /// # 返回
    /// 断言去重后字典序稳定输出.
    ///
    /// # 错误
    /// 去重或排序回归时测试失败.
    ///
    /// # 约束
    /// 查询次数固定，不随页大小增长；总数与排序由集合查询保证.
    #[test]
    fn duplicate_references_are_deduplicated_and_sorted() {
        let rules = vec![
            test_rule(
                "r-2",
                ProcurementResponsibilityRuleType::Sku,
                Some("sku-b"),
                None,
                "owner-b",
            ),
            test_rule(
                "r-1",
                ProcurementResponsibilityRuleType::Sku,
                Some("sku-a"),
                None,
                "owner-a",
            ),
            test_rule(
                "r-3",
                ProcurementResponsibilityRuleType::Sku,
                Some("sku-a"),
                None,
                "owner-a",
            ),
        ];
        let (owners, skus, categories) = collect_rule_list_ids(&rules);
        assert_eq!(owners, vec!["owner-a".to_string(), "owner-b".to_string()]);
        assert_eq!(skus, vec![SkuId::new("sku-a"), SkuId::new("sku-b")]);
        assert!(categories.is_empty());
    }

    /// 分类引用同样去重排序，缺失由稀疏映射保持空展示.
    ///
    /// # 参数
    /// 无.
    ///
    /// # 返回
    /// 断言分类 ID 去重排序结果.
    ///
    /// # 错误
    /// 收集逻辑回归时测试失败.
    ///
    /// # 约束
    /// 缺项不报错，由 Service 保留空展示；软删除由 base 查询过滤.
    #[test]
    fn category_references_are_deduplicated_and_sorted() {
        let rules = vec![
            test_rule(
                "r-1",
                ProcurementResponsibilityRuleType::Category,
                None,
                Some("cat-b"),
                "owner-1",
            ),
            test_rule(
                "r-2",
                ProcurementResponsibilityRuleType::Category,
                None,
                Some("cat-a"),
                "owner-1",
            ),
        ];
        let (_, skus, categories) = collect_rule_list_ids(&rules);
        assert!(skus.is_empty());
        assert_eq!(
            categories,
            vec![ProductCategoryId::new("cat-a"), ProductCategoryId::new("cat-b")]
        );
    }
}
