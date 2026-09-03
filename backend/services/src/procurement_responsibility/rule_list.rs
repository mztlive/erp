//! 采购责任规则管理列表的纯响应映射.
//!
//! Repository 负责分页、总数、排序、软删除与批量关联事实；本模块只解释持久化事实并映射为 API 视图.

use super::dto::ProcurementResponsibilityRuleView;
use entities::procurement_responsibility::{ProcurementResponsibilityRule, ProcurementRuleListDisplayFacts};

/// 将规则分页读模型映射为管理列表视图.
///
/// # 参数
/// * `items` - 当前页规则实体，按优先级与创建时间稳定排序
/// * `facts` - 当前页展示所需的最小关联事实
///
/// # 返回
/// 返回保持输入顺序的管理列表视图；缺失历史引用时对应展示字段保持 `None`.
///
/// # 错误
/// 无；纯内存映射，不访问数据库.
///
/// # 约束
/// 不改变软删除、总数、分页、排序语义；缺失引用不得报错，必须保持空展示.
pub fn to_rule_list_views(
    items: Vec<ProcurementResponsibilityRule>,
    facts: &ProcurementRuleListDisplayFacts,
) -> Vec<ProcurementResponsibilityRuleView> {
    let mut views = items.into_iter().map(Into::into).collect::<Vec<_>>();
    apply_rule_list_facts(&mut views, facts);
    views
}

/// 原地补全规则视图的负责人、SKU 与分类展示字段.
///
/// # 参数
/// * `views` - 已由规则实体映射的当前页视图
/// * `facts` - Repository 批量返回的稀疏展示事实
///
/// # 返回
/// 原地补全可解析的展示字段后返回 `()`；引用已删除时保留空展示.
///
/// # 错误
/// 无；缺失键保持 `None`，不访问数据库.
///
/// # 约束
/// 仅做映射，不做缺失校验、排序或总数调整；展示姓名字段不得参与领域身份.
pub fn apply_rule_list_facts(
    views: &mut [ProcurementResponsibilityRuleView],
    facts: &ProcurementRuleListDisplayFacts,
) {
    for view in views {
        view.owner_name = facts.owner_names.get(&view.owner_user_id).cloned();
        if let Some(sku_id) = view.sku_id.as_ref() {
            let key = sku_id.as_ref();
            if let Some(sku_no) = facts.sku_nos.get(key) {
                view.sku_no = Some(sku_no.clone());
            }
            view.sku_name = facts.sku_names.get(key).cloned();
        }
        if let Some(category_id) = view.category_id.as_ref() {
            view.category_name = facts.category_names.get(category_id.as_ref()).cloned();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use entities::catalog::EnableStatus;
    use entities::ids::{ProcurementResponsibilityRuleId, ProductCategoryId, SkuId};
    use entities::procurement_responsibility::{
        ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
    };

    use super::*;

    fn sku_rule(id: &str, sku: &str, category: Option<&str>, owner: &str) -> ProcurementResponsibilityRule {
        let (rule_type, category_id) = if let Some(category) = category {
            (
                ProcurementResponsibilityRuleType::Category,
                Some(ProductCategoryId::new(category)),
            )
        } else {
            (ProcurementResponsibilityRuleType::Sku, None)
        };
        ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new(id),
            ProcurementResponsibilityRuleData {
                rule_type,
                sku_id: if category.is_some() {
                    None
                } else {
                    Some(SkuId::new(sku))
                },
                category_id,
                service_region: None,
                product_kind: None,
                owner_user_id: owner.to_string(),
                status: EnableStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    fn test_facts() -> ProcurementRuleListDisplayFacts {
        ProcurementRuleListDisplayFacts {
            owner_names: HashMap::from([("owner-1".to_string(), "张三".to_string())]),
            sku_nos: HashMap::from([("sku-1".to_string(), "SKU-001".to_string())]),
            sku_names: HashMap::from([("sku-1".to_string(), "红色零件".to_string())]),
            category_names: HashMap::from([("cat-1".to_string(), "五金".to_string())]),
        }
    }

    /// 完整事实映射展示字段并保持输入顺序.
    ///
    /// # 参数
    /// 无.
    ///
    /// # 返回
    /// 断言顺序与展示字段.
    ///
    /// # 错误
    /// 映射回归时测试失败.
    ///
    /// # 约束
    /// 不改变分页与排序；仅补展示字段.
    #[test]
    fn full_facts_fill_display_fields_in_order() {
        let facts = test_facts();
        let views = to_rule_list_views(
            vec![
                sku_rule("r-1", "sku-1", None, "owner-1"),
                sku_rule("r-2", "sku-1", Some("cat-1"), "owner-1"),
            ],
            &facts,
        );
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].owner_name.as_deref(), Some("张三"));
        assert_eq!(views[0].sku_no.as_deref(), Some("SKU-001"));
        assert_eq!(views[0].sku_name.as_deref(), Some("红色零件"));
        assert_eq!(views[1].category_name.as_deref(), Some("五金"));
    }

    /// 缺失历史引用保持空展示而不报错.
    ///
    /// # 参数
    /// 无.
    ///
    /// # 返回
    /// 断言缺失键展示为 `None`.
    ///
    /// # 错误
    /// 空展示语义漂移时测试失败.
    ///
    /// # 约束
    /// 缺项不得报错；软删除与总数语义由 Repository 保证.
    #[test]
    fn missing_references_keep_empty_display() {
        let facts = ProcurementRuleListDisplayFacts::default();
        let views = to_rule_list_views(vec![sku_rule("r-9", "sku-gone", None, "owner-gone")], &facts);
        assert_eq!(views.len(), 1);
        assert!(views[0].owner_name.is_none());
        assert!(views[0].sku_no.is_none());
        assert!(views[0].sku_name.is_none());
        assert!(views[0].category_name.is_none());
    }

    /// SKU 存在但当前修订缺失时编号保留而名称为空.
    ///
    /// # 参数
    /// 无.
    ///
    /// # 返回
    /// 断言编号填充且名称为空.
    ///
    /// # 错误
    /// 稀疏语义回归时测试失败.
    ///
    /// # 约束
    /// 修订缺失不得清空 SKU 编号展示.
    #[test]
    fn sku_without_current_revision_keeps_number_without_name() {
        let facts = ProcurementRuleListDisplayFacts {
            sku_nos: HashMap::from([("sku-1".to_string(), "SKU-001".to_string())]),
            ..ProcurementRuleListDisplayFacts::default()
        };
        let views = to_rule_list_views(vec![sku_rule("r-1", "sku-1", None, "owner-1")], &facts);
        assert_eq!(views[0].sku_no.as_deref(), Some("SKU-001"));
        assert!(views[0].sku_name.is_none());
    }
}
