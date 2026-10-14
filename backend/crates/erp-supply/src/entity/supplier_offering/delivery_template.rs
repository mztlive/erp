//! 供应商配送模板：按地区规定代发能否配送、集采包邮/运费另计/不配送。
//!
//! 模板归供应商所有，一个供应商可有多个模板、至多一个默认模板。供给引用模板，
//! 未引用时跟随供应商默认模板；二者都没有时按全国配送、包邮判定。模板每次
//! 修改推进乐观锁版本，版本号即模板版本；采购单冻结判定结果，不随模板变化。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::Result;
use erp_core::delivery_policy::{DeliveryPolicy, DeliveryRuleSet};
use erp_core::ids::{SupplierAccountId, SupplierDeliveryTemplateId};
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use super::SupplierOffering;

const TEMPLATE_NAME_MAX_LEN: usize = 64;

/// 模板可编辑内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierDeliveryTemplateContent {
    /// 模板名称，同一供应商内唯一。
    pub name: String,
    /// 是否为供应商默认模板。
    pub is_default: bool,
    /// 代发规则。
    pub dropship: DeliveryRuleSet,
    /// 集采规则。
    pub bulk: DeliveryRuleSet,
}

/// 供应商配送模板。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SupplierDeliveryTemplate {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属供应商；创建后不可修改。
    pub supplier_id: SupplierAccountId,
    /// 模板名称。
    pub name: String,
    /// 是否为供应商默认模板。
    pub is_default: bool,
    /// 代发与集采规则。
    pub policy: DeliveryPolicy,
    /// 创建人。
    pub created_by: String,
    /// 最后修改人。
    pub updated_by: String,
}

impl SupplierDeliveryTemplate {
    /// 校验并创建模板。
    ///
    /// # 参数
    /// * `id` - 模板主键
    /// * `supplier_id` - 所属供应商
    /// * `content` - 名称、默认标记与规则
    /// * `actor` - 创建人
    ///
    /// # 返回
    /// 返回规范化后的模板。
    ///
    /// # 错误
    /// 名称为空或超长、规则不合法时返回错误。名称唯一与默认唯一由 Service 在事务内校验。
    pub fn new(
        id: SupplierDeliveryTemplateId,
        supplier_id: SupplierAccountId,
        content: SupplierDeliveryTemplateContent,
        actor: &str,
    ) -> Result<Self> {
        let (name, policy) = normalize_content(content.name, content.dropship, content.bulk)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_id,
            name,
            is_default: content.is_default,
            policy,
            created_by: actor.to_string(),
            updated_by: actor.to_string(),
        })
    }

    /// 整体替换模板内容。
    ///
    /// # 参数
    /// * `content` - 新的名称、默认标记与规则
    /// * `actor` - 修改人
    ///
    /// # 返回
    /// 内容有变化时返回 `true`。
    ///
    /// # 错误
    /// 名称为空或超长、规则不合法时返回错误。
    pub fn replace_content(&mut self, content: SupplierDeliveryTemplateContent, actor: &str) -> Result<bool> {
        let (name, policy) = normalize_content(content.name, content.dropship, content.bulk)?;
        if name == self.name && content.is_default == self.is_default && policy == self.policy {
            return Ok(false);
        }
        self.name = name;
        self.is_default = content.is_default;
        self.policy = policy;
        self.updated_by = actor.to_string();
        Ok(true)
    }

    /// 取消默认标记，供另一模板设为默认时调用。
    ///
    /// # 参数
    /// * `actor` - 修改人
    ///
    /// # 返回
    /// 原为默认时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn clear_default(&mut self, actor: &str) -> bool {
        if !self.is_default {
            return false;
        }
        self.is_default = false;
        self.updated_by = actor.to_string();
        true
    }

    /// 返回模板版本号，即乐观锁版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前版本号。
    ///
    /// # 错误
    /// 无。
    pub fn revision_no(&self) -> u64 {
        self.base.version
    }
}

/// 供给配送模板的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryTemplateSource {
    /// 供给指定的模板。
    Assigned,
    /// 跟随供应商默认模板。
    SupplierDefault,
    /// 未设置，按全国配送、包邮判定。
    Unset,
}

/// 供给当前生效的配送政策及其来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDelivery {
    /// 来源。
    pub source: DeliveryTemplateSource,
    /// 生效模板主键；未设置时为空。
    pub template_id: Option<SupplierDeliveryTemplateId>,
    /// 生效模板名称；未设置时为空。
    pub template_name: Option<String>,
    /// 生效模板版本号；未设置时为空。
    pub template_revision_no: Option<u64>,
    /// 生效配送政策。
    pub policy: DeliveryPolicy,
}

impl ResolvedDelivery {
    /// 返回界面展示的模板名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 指定模板返回模板名；跟随默认返回"供应商默认（模板名）"；未设置返回固定说明。
    ///
    /// # 错误
    /// 无。
    pub fn display_name(&self) -> String {
        match (self.source, self.template_name.as_deref()) {
            (DeliveryTemplateSource::Assigned, Some(name)) => name.to_string(),
            (DeliveryTemplateSource::SupplierDefault, Some(name)) => format!("供应商默认（{name}）"),
            _ => "未设置（全国配送、包邮）".to_string(),
        }
    }

    fn from_template(source: DeliveryTemplateSource, template: &SupplierDeliveryTemplate) -> Self {
        Self {
            source,
            template_id: Some(SupplierDeliveryTemplateId::new(template.base.id.clone())),
            template_name: Some(template.name.clone()),
            template_revision_no: Some(template.revision_no()),
            policy: template.policy.clone(),
        }
    }
}

/// 解析供给当前生效的配送政策。
///
/// # 参数
/// * `offering` - 供给稳定对象
/// * `templates` - 已加载的模板，至少包含该供应商全部未删除模板
///
/// # 返回
/// 指定模板存在且属于同一供应商时取指定模板；否则取供应商默认模板；
/// 二者都没有时返回全国配送、包邮的政策。
///
/// # 错误
/// 无。
pub fn resolve_offering_delivery(
    offering: &SupplierOffering,
    templates: &[SupplierDeliveryTemplate],
) -> ResolvedDelivery {
    resolve_delivery(&offering.supplier_id, offering.delivery_template_id.as_ref(), templates)
}

/// 按供应商与指定模板解析生效配送政策。
///
/// # 参数
/// * `supplier_id` - 供给所属供应商
/// * `assigned` - 供给指定的模板
/// * `templates` - 已加载的模板，至少包含该供应商全部未删除模板
///
/// # 返回
/// 规则同 [`resolve_offering_delivery`]。
///
/// # 错误
/// 无。
pub fn resolve_delivery(
    supplier_id: &SupplierAccountId,
    assigned: Option<&SupplierDeliveryTemplateId>,
    templates: &[SupplierDeliveryTemplate],
) -> ResolvedDelivery {
    let same_supplier = |template: &&SupplierDeliveryTemplate| template.supplier_id == *supplier_id;
    let assigned = assigned.and_then(|id| {
        templates.iter().filter(same_supplier).find(|template| template.base.id == id.as_ref())
    });
    if let Some(template) = assigned {
        return ResolvedDelivery::from_template(DeliveryTemplateSource::Assigned, template);
    }
    if let Some(template) = templates.iter().filter(same_supplier).find(|template| template.is_default) {
        return ResolvedDelivery::from_template(DeliveryTemplateSource::SupplierDefault, template);
    }
    ResolvedDelivery {
        source: DeliveryTemplateSource::Unset,
        template_id: None,
        template_name: None,
        template_revision_no: None,
        policy: DeliveryPolicy::nationwide_included(),
    }
}

fn normalize_content(
    name: String,
    dropship: DeliveryRuleSet,
    bulk: DeliveryRuleSet,
) -> Result<(String, DeliveryPolicy)> {
    let name = normalize_required_text(name, "请填写模板名称", TEMPLATE_NAME_MAX_LEN, "模板名称过长")?;
    Ok((name, DeliveryPolicy::new(dropship, bulk)?))
}

#[cfg(test)]
mod tests {
    use erp_core::delivery_policy::{DeliveryMode, DeliveryRegionRule, DeliveryRuleSet, DeliveryTerm};
    use erp_core::ids::{SupplierAccountId, SupplierDeliveryTemplateId};
    use erp_core::region::RegionCode;

    use super::{
        DeliveryTemplateSource, SupplierDeliveryTemplate, SupplierDeliveryTemplateContent,
        resolve_offering_delivery,
    };
    use crate::entity::supplier_offering::{OfferingSourceType, SupplierOffering, SupplierOfferingData};

    fn content(name: &str) -> SupplierDeliveryTemplateContent {
        SupplierDeliveryTemplateContent {
            name: name.to_string(),
            is_default: true,
            dropship: DeliveryRuleSet {
                default_term: DeliveryTerm::FreightIncluded,
                rules: vec![DeliveryRegionRule {
                    region_code: RegionCode::parse("650000").unwrap(),
                    term: DeliveryTerm::NotDeliverable,
                }],
            },
            bulk: DeliveryRuleSet {
                default_term: DeliveryTerm::FreightExtra,
                rules: vec![DeliveryRegionRule {
                    region_code: RegionCode::parse("440000").unwrap(),
                    term: DeliveryTerm::FreightIncluded,
                }],
            },
        }
    }

    fn template() -> SupplierDeliveryTemplate {
        SupplierDeliveryTemplate::new(
            SupplierDeliveryTemplateId::new("tpl-1"),
            SupplierAccountId::new("supplier-1"),
            content(" 常温 "),
            "buyer",
        )
        .unwrap()
    }

    #[test]
    fn new_normalizes_name_and_keeps_policy() {
        let template = template();
        assert_eq!(template.name, "常温");
        assert!(template.is_default);
        let guangzhou = RegionCode::parse("440106").unwrap();
        assert_eq!(template.policy.judge(DeliveryMode::Bulk, &guangzhou), DeliveryTerm::FreightIncluded);
    }

    #[test]
    fn new_rejects_blank_name_and_invalid_policy() {
        let blank = SupplierDeliveryTemplate::new(
            SupplierDeliveryTemplateId::new("tpl-1"),
            SupplierAccountId::new("supplier-1"),
            content("  "),
            "buyer",
        );
        assert!(blank.is_err());
        let mut invalid = content("常温");
        invalid.dropship.default_term = DeliveryTerm::FreightExtra;
        let result = SupplierDeliveryTemplate::new(
            SupplierDeliveryTemplateId::new("tpl-1"),
            SupplierAccountId::new("supplier-1"),
            invalid,
            "buyer",
        );
        assert!(result.unwrap_err().to_string().contains("代发规则只能选择配送或不配送"));
    }

    #[test]
    fn replace_content_reports_whether_anything_changed() {
        let mut template = template();
        assert!(!template.replace_content(content("常温"), "other").unwrap());
        assert_eq!(template.updated_by, "buyer");
        let mut changed = content("常温");
        changed.bulk.default_term = DeliveryTerm::NotDeliverable;
        assert!(template.replace_content(changed, "other").unwrap());
        assert_eq!(template.updated_by, "other");
    }

    #[test]
    fn clear_default_only_changes_default_templates() {
        let mut template = template();
        assert!(template.clear_default("other"));
        assert!(!template.is_default);
        assert!(!template.clear_default("other"));
    }

    fn offering(supplier: &str, template: Option<&str>) -> SupplierOffering {
        let mut offering = SupplierOffering::new(
            erp_core::ids::SupplierOfferingId::new("offering-1"),
            SupplierOfferingData {
                sku_id: erp_core::ids::SkuId::new("sku-1"),
                supplier_id: SupplierAccountId::new(supplier),
                supplier_product_code: None,
                supplier_sku_code: "S-1".into(),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
                maintainer_user_id: "buyer".into(),
                business_org_unit_id: "org".into(),
            },
            "buyer",
        )
        .unwrap();
        offering.delivery_template_id = template.map(SupplierDeliveryTemplateId::new);
        offering
    }

    fn named(id: &str, supplier: &str, is_default: bool) -> SupplierDeliveryTemplate {
        let mut value = content(id);
        value.is_default = is_default;
        SupplierDeliveryTemplate::new(
            SupplierDeliveryTemplateId::new(id),
            SupplierAccountId::new(supplier),
            value,
            "buyer",
        )
        .unwrap()
    }

    #[test]
    fn resolve_prefers_assigned_then_supplier_default_then_nationwide() {
        let templates = vec![
            named("cold", "supplier-1", false),
            named("normal", "supplier-1", true),
            named("x", "other", true),
        ];
        let assigned = resolve_offering_delivery(&offering("supplier-1", Some("cold")), &templates);
        assert_eq!(assigned.source, DeliveryTemplateSource::Assigned);
        assert_eq!(assigned.display_name(), "cold");
        let fallback = resolve_offering_delivery(&offering("supplier-1", None), &templates);
        assert_eq!(fallback.source, DeliveryTemplateSource::SupplierDefault);
        assert_eq!(fallback.display_name(), "供应商默认（normal）");
        let unset = resolve_offering_delivery(&offering("supplier-2", None), &templates);
        assert_eq!(unset.source, DeliveryTemplateSource::Unset);
        assert_eq!(unset.display_name(), "未设置（全国配送、包邮）");
        assert!(unset.template_revision_no.is_none());
    }

    #[test]
    fn resolve_ignores_templates_of_other_suppliers() {
        let templates = vec![named("x", "other", false)];
        let resolved = resolve_offering_delivery(&offering("supplier-1", Some("x")), &templates);
        assert_eq!(resolved.source, DeliveryTemplateSource::Unset);
    }
}
