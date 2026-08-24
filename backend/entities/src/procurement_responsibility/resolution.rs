//! 采购责任上下文与确定性规则优先级解析。

use std::collections::HashSet;

use crate::catalog::ProductKind;
use crate::errors::{Error, Result};
use crate::ids::{ProductCategoryId, SkuId};

use super::rule::{
    normalize_service_region, ProcurementResponsibilityRule, ProcurementResponsibilityRuleType,
};

/// 单条采购需求行的责任解析事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementResponsibilityContext {
    /// 精确 SKU。
    pub sku_id: SkuId,
    /// 从当前分类开始到根分类的有序链。
    pub category_chain: Vec<ProductCategoryId>,
    /// 已规范化的可选服务区域。
    pub service_region: Option<String>,
    /// 商品业务类型。
    pub product_kind: ProductKind,
}

impl ProcurementResponsibilityContext {
    /// 创建规范化责任解析上下文。
    ///
    /// # 参数
    /// * `sku_id` - 精确 SKU
    /// * `category_chain` - 当前分类到根分类的有序链
    /// * `service_region` - 可选服务区域
    /// * `product_kind` - 商品业务类型
    ///
    /// # 返回
    /// 返回服务区域已规范化的解析上下文。
    ///
    /// # 错误
    /// 分类链为空、重复或服务区域过长时返回错误。
    pub fn new(
        sku_id: SkuId,
        category_chain: Vec<ProductCategoryId>,
        service_region: Option<String>,
        product_kind: ProductKind,
    ) -> Result<Self> {
        ensure_category_chain(&category_chain)?;
        Ok(Self {
            sku_id,
            category_chain,
            service_region: normalize_service_region(service_region)?,
            product_kind,
        })
    }
}

/// 启用采购责任规则的确定性解析集合。
pub struct ProcurementResponsibilityRuleSet<'a> {
    rules: Vec<&'a ProcurementResponsibilityRule>,
}

impl<'a> ProcurementResponsibilityRuleSet<'a> {
    /// 从规则实体构造只包含启用规则的解析集合。
    ///
    /// # 参数
    /// * `rules` - 当前可见规则实体
    ///
    /// # 返回
    /// 返回保持输入顺序但解析结果不依赖该顺序的规则集合。
    ///
    /// # 错误
    /// 无。
    pub fn new(rules: &'a [ProcurementResponsibilityRule]) -> Self {
        Self {
            rules: rules.iter().filter(|rule| rule.is_active()).collect(),
        }
    }

    /// 按合同优先级解析唯一具体责任规则。
    ///
    /// # 参数
    /// * `context` - SKU、分类链、服务区域与 ProductKind 事实
    ///
    /// # 返回
    /// 返回唯一命中的规则引用。
    ///
    /// # 错误
    /// 任一同层多命中、分类链非法或最终没有默认调度人时返回错误；高层零命中才继续低层。
    pub fn resolve(
        &self,
        context: &ProcurementResponsibilityContext,
    ) -> Result<&'a ProcurementResponsibilityRule> {
        if let Some(rule) = self.unique_match(ProcurementResponsibilityRuleType::Sku, |rule| {
            rule.sku_id.as_ref() == Some(&context.sku_id)
        })? {
            return Ok(rule);
        }
        if let Some(rule) = self.resolve_category_region(context)? {
            return Ok(rule);
        }
        if let Some(rule) = self.resolve_category_chain(&context.category_chain)? {
            return Ok(rule);
        }
        if let Some(rule) = self.unique_match(ProcurementResponsibilityRuleType::ProductKind, |rule| {
            rule.product_kind == Some(context.product_kind)
        })? {
            return Ok(rule);
        }
        self.unique_match(ProcurementResponsibilityRuleType::DefaultDispatcher, |_| true)?
            .ok_or_else(|| Error::from("采购责任解析失败：未配置具体默认调度人"))
    }

    /// 解析当前分类与服务区域的精确规则层。
    ///
    /// # 参数
    /// * `context` - 当前行责任事实
    ///
    /// # 返回
    /// 无服务区域或零命中返回 `None`，唯一命中返回规则。
    ///
    /// # 错误
    /// 同层多命中时返回错误。
    fn resolve_category_region(
        &self,
        context: &ProcurementResponsibilityContext,
    ) -> Result<Option<&'a ProcurementResponsibilityRule>> {
        let Some(region) = context.service_region.as_deref() else {
            return Ok(None);
        };
        let category_id = context.category_chain.first().expect("构造已保证分类链非空");
        self.unique_match(ProcurementResponsibilityRuleType::CategoryServiceRegion, |rule| {
            rule.category_id.as_ref() == Some(category_id) && rule.service_region.as_deref() == Some(region)
        })
    }

    /// 按当前分类到父分类的顺序逐级解析分类规则。
    ///
    /// # 参数
    /// * `category_chain` - 当前分类到根分类的有序链
    ///
    /// # 返回
    /// 返回首个层级的唯一命中；全链零命中返回 `None`。
    ///
    /// # 错误
    /// 任一分类层多命中时返回错误。
    fn resolve_category_chain(
        &self,
        category_chain: &[ProductCategoryId],
    ) -> Result<Option<&'a ProcurementResponsibilityRule>> {
        for category_id in category_chain {
            let matched = self.unique_match(ProcurementResponsibilityRuleType::Category, |rule| {
                rule.category_id.as_ref() == Some(category_id)
            })?;
            if matched.is_some() {
                return Ok(matched);
            }
        }
        Ok(None)
    }

    /// 在一个规则层内要求零或唯一命中。
    ///
    /// # 参数
    /// * `rule_type` - 当前规则层
    /// * `predicate` - 选择器匹配条件
    ///
    /// # 返回
    /// 零命中返回 `None`，唯一命中返回规则。
    ///
    /// # 错误
    /// 同层多命中时返回失败关闭错误。
    fn unique_match<F>(
        &self,
        rule_type: ProcurementResponsibilityRuleType,
        predicate: F,
    ) -> Result<Option<&'a ProcurementResponsibilityRule>>
    where
        F: Fn(&ProcurementResponsibilityRule) -> bool,
    {
        let mut matched = self
            .rules
            .iter()
            .copied()
            .filter(|rule| rule.rule_type == rule_type && predicate(rule));
        let first = matched.next();
        if first.is_some() && matched.next().is_some() {
            return Err(Error::from(format!(
                "采购责任解析失败：{} 同层命中多条启用规则",
                rule_type.as_str()
            )));
        }
        Ok(first)
    }
}

/// 校验分类链非空且不重复。
///
/// # 参数
/// * `category_chain` - 当前分类到根分类的有序链
///
/// # 返回
/// 分类链可用于父级回退时返回 `Ok(())`。
///
/// # 错误
/// 分类链为空或出现重复节点时返回错误。
fn ensure_category_chain(category_chain: &[ProductCategoryId]) -> Result<()> {
    if category_chain.is_empty() {
        return Err(Error::from("采购责任解析缺少商品分类"));
    }
    let mut seen = HashSet::new();
    if category_chain
        .iter()
        .any(|category| !seen.insert(category.as_ref()))
    {
        return Err(Error::from("采购责任解析发现分类父级环"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::catalog::EnableStatus;
    use crate::ids::ProcurementResponsibilityRuleId;
    use crate::procurement_responsibility::ProcurementResponsibilityRuleData;

    use super::*;

    fn rule(
        id: &str,
        rule_type: ProcurementResponsibilityRuleType,
        sku_id: Option<&str>,
        category_id: Option<&str>,
        region: Option<&str>,
        product_kind: Option<ProductKind>,
        owner: &str,
    ) -> ProcurementResponsibilityRule {
        ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new(id),
            ProcurementResponsibilityRuleData {
                rule_type,
                sku_id: sku_id.map(SkuId::new),
                category_id: category_id.map(ProductCategoryId::new),
                service_region: region.map(ToString::to_string),
                product_kind,
                owner_user_id: owner.to_string(),
                status: EnableStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    fn context() -> ProcurementResponsibilityContext {
        ProcurementResponsibilityContext::new(
            SkuId::new("sku-1"),
            vec![
                ProductCategoryId::new("cat-child"),
                ProductCategoryId::new("cat-parent"),
            ],
            Some(" north ".to_string()),
            ProductKind::Physical,
        )
        .unwrap()
    }

    #[test]
    fn resolver_obeys_priority_and_parent_fallback() {
        let rules = vec![
            rule(
                "default",
                ProcurementResponsibilityRuleType::DefaultDispatcher,
                None,
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
                None,
                Some(ProductKind::Physical),
                "kind-owner",
            ),
            rule(
                "parent",
                ProcurementResponsibilityRuleType::Category,
                None,
                Some("cat-parent"),
                None,
                None,
                "parent-owner",
            ),
            rule(
                "region",
                ProcurementResponsibilityRuleType::CategoryServiceRegion,
                None,
                Some("cat-child"),
                Some("NORTH"),
                None,
                "region-owner",
            ),
            rule(
                "sku",
                ProcurementResponsibilityRuleType::Sku,
                Some("sku-1"),
                None,
                None,
                None,
                "sku-owner",
            ),
        ];
        let set = ProcurementResponsibilityRuleSet::new(&rules);
        assert_eq!(set.resolve(&context()).unwrap().owner_user_id, "sku-owner");

        let without_sku = ProcurementResponsibilityRuleSet::new(&rules[..4]);
        assert_eq!(
            without_sku.resolve(&context()).unwrap().owner_user_id,
            "region-owner"
        );

        let parent_only = vec![rules[0].clone(), rules[1].clone(), rules[2].clone()];
        assert_eq!(
            ProcurementResponsibilityRuleSet::new(&parent_only)
                .resolve(&context())
                .unwrap()
                .owner_user_id,
            "parent-owner"
        );

        let kind_only = vec![rules[0].clone(), rules[1].clone()];
        assert_eq!(
            ProcurementResponsibilityRuleSet::new(&kind_only)
                .resolve(&context())
                .unwrap()
                .owner_user_id,
            "kind-owner"
        );
        assert_eq!(
            ProcurementResponsibilityRuleSet::new(&rules[..1])
                .resolve(&context())
                .unwrap()
                .owner_user_id,
            "default-owner"
        );
    }

    #[test]
    fn category_region_does_not_fall_back_to_parent_category() {
        let rules = vec![
            rule(
                "default",
                ProcurementResponsibilityRuleType::DefaultDispatcher,
                None,
                None,
                None,
                None,
                "default-owner",
            ),
            rule(
                "parent-region",
                ProcurementResponsibilityRuleType::CategoryServiceRegion,
                None,
                Some("cat-parent"),
                Some("NORTH"),
                None,
                "parent-region-owner",
            ),
        ];

        assert_eq!(
            ProcurementResponsibilityRuleSet::new(&rules)
                .resolve(&context())
                .unwrap()
                .owner_user_id,
            "default-owner"
        );
    }

    #[test]
    fn resolver_fails_closed_for_conflicts_invalid_chain_and_missing_default() {
        let duplicate_defaults = vec![
            rule(
                "d-1",
                ProcurementResponsibilityRuleType::DefaultDispatcher,
                None,
                None,
                None,
                None,
                "buyer-1",
            ),
            rule(
                "d-2",
                ProcurementResponsibilityRuleType::DefaultDispatcher,
                None,
                None,
                None,
                None,
                "buyer-2",
            ),
        ];
        assert!(ProcurementResponsibilityRuleSet::new(&duplicate_defaults)
            .resolve(&context())
            .unwrap_err()
            .to_string()
            .contains("多条"));
        assert!(ProcurementResponsibilityRuleSet::new(&[])
            .resolve(&context())
            .unwrap_err()
            .to_string()
            .contains("默认调度人"));
        assert!(ProcurementResponsibilityContext::new(
            SkuId::new("sku-1"),
            Vec::new(),
            None,
            ProductKind::Physical,
        )
        .is_err());
        assert!(ProcurementResponsibilityContext::new(
            SkuId::new("sku-1"),
            vec![ProductCategoryId::new("cat-1"), ProductCategoryId::new("cat-1")],
            None,
            ProductKind::Physical,
        )
        .is_err());
    }
}
