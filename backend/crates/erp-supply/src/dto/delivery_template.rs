//! 配送模板请求与视图合同。

use erp_core::delivery_policy::{DeliveryRegionRule, DeliveryRuleSet, DeliveryTerm};
use erp_core::region::RegionCode;
use serde::{Deserialize, Serialize};

use crate::entity::supplier_offering::{
    DeliveryTemplateSource, ResolvedDelivery, SupplierDeliveryTemplate, SupplierDeliveryTemplateContent,
};
use crate::{Error, Result};

/// 一次批量指定最多的供给数。
pub const MAX_ASSIGN_OFFERINGS: usize = 200;

/// 地区例外输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRegionRuleInput {
    /// 省、市或区县代码。
    pub region_code: String,
    /// 判定。
    pub term: DeliveryTerm,
}

/// 一种供货方式的规则输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRuleSetInput {
    /// 其他地区的判定。
    pub default_term: DeliveryTerm,
    /// 地区例外。
    #[serde(default)]
    pub rules: Vec<DeliveryRegionRuleInput>,
}

impl DeliveryRuleSetInput {
    fn into_rule_set(self) -> Result<DeliveryRuleSet> {
        let rules = self
            .rules
            .into_iter()
            .map(|rule| {
                Ok(DeliveryRegionRule { region_code: RegionCode::parse(&rule.region_code)?, term: rule.term })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(DeliveryRuleSet { default_term: self.default_term, rules })
    }
}

/// 新建或整体保存模板。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveDeliveryTemplateRequest {
    /// 模板名称。
    pub name: String,
    /// 是否设为供应商默认模板。
    #[serde(default)]
    pub is_default: bool,
    /// 代发规则。
    pub dropship: DeliveryRuleSetInput,
    /// 集采规则。
    pub bulk: DeliveryRuleSetInput,
    /// 修改时必填的期望版本；新建时忽略。
    #[serde(default)]
    pub expected_version: Option<u64>,
}

impl SaveDeliveryTemplateRequest {
    /// 转换为模板内容并解析地区代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回待实体校验的模板内容。
    ///
    /// # 错误
    /// 地区代码格式非法或不存在时返回校验错误。
    pub fn into_content(self) -> Result<SupplierDeliveryTemplateContent> {
        Ok(SupplierDeliveryTemplateContent {
            name: self.name,
            is_default: self.is_default,
            dropship: self.dropship.into_rule_set().map_err(validation)?,
            bulk: self.bulk.into_rule_set().map_err(validation)?,
        })
    }
}

/// 删除模板。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteDeliveryTemplateRequest {
    /// 期望版本。
    pub expected_version: u64,
}

/// 为供给指定配送模板。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignDeliveryTemplateRequest {
    /// 同一供应商的供给。
    pub offering_ids: Vec<String>,
    /// 目标模板；为空表示跟随供应商默认模板。
    #[serde(default)]
    pub template_id: Option<String>,
}

impl AssignDeliveryTemplateRequest {
    /// 去重并校验供给数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回保持首次出现顺序的供给主键。
    ///
    /// # 错误
    /// 为空或超过上限时返回校验错误。
    pub fn normalized_offering_ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for id in self.offering_ids.iter().map(|id| id.trim()).filter(|id| !id.is_empty()) {
            if !ids.iter().any(|existing: &String| existing == id) {
                ids.push(id.to_string());
            }
        }
        if ids.is_empty() {
            return Err(Error::ValidationError("请选择供给".into()));
        }
        if ids.len() > MAX_ASSIGN_OFFERINGS {
            return Err(Error::ValidationError(format!("一次最多为 {MAX_ASSIGN_OFFERINGS} 条供给指定模板")));
        }
        Ok(ids)
    }
}

/// 批量指定结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignDeliveryTemplateResult {
    /// 实际发生变化的供给数。
    pub updated_count: usize,
}

/// 地区例外视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRegionRuleView {
    /// 地区代码。
    pub region_code: String,
    /// 完整地区名称。
    pub region_name: String,
    /// 判定。
    pub term: DeliveryTerm,
}

/// 一种供货方式的规则视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRuleSetView {
    /// 其他地区的判定。
    pub default_term: DeliveryTerm,
    /// 地区例外。
    pub rules: Vec<DeliveryRegionRuleView>,
}

impl DeliveryRuleSetView {
    /// 由规则值对象生成视图。
    ///
    /// # 参数
    /// * `rule_set` - 规则
    ///
    /// # 返回
    /// 返回带完整地区名称的视图。
    ///
    /// # 错误
    /// 无。
    pub fn from_rule_set(rule_set: &DeliveryRuleSet) -> Self {
        Self {
            default_term: rule_set.default_term,
            rules: rule_set
                .rules
                .iter()
                .map(|rule| DeliveryRegionRuleView {
                    region_code: rule.region_code.to_string(),
                    region_name: rule.region_code.full_name(),
                    term: rule.term,
                })
                .collect(),
        }
    }
}

/// 配送模板视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryTemplateView {
    /// 模板主键。
    pub id: String,
    /// 供应商。
    pub supplier_id: String,
    /// 名称。
    pub name: String,
    /// 是否默认。
    pub is_default: bool,
    /// 版本号。
    pub version: u64,
    /// 代发规则。
    pub dropship: DeliveryRuleSetView,
    /// 集采规则。
    pub bulk: DeliveryRuleSetView,
    /// 最后修改人。
    pub updated_by: String,
    /// 最后修改时间（毫秒）。
    pub updated_at: u64,
}

impl DeliveryTemplateView {
    /// 由模板实体生成视图。
    ///
    /// # 参数
    /// * `template` - 模板
    ///
    /// # 返回
    /// 返回模板视图。
    ///
    /// # 错误
    /// 无。
    pub fn from_template(template: &SupplierDeliveryTemplate) -> Self {
        Self {
            id: template.base.id.clone(),
            supplier_id: template.supplier_id.to_string(),
            name: template.name.clone(),
            is_default: template.is_default,
            version: template.base.version,
            dropship: DeliveryRuleSetView::from_rule_set(&template.policy.dropship),
            bulk: DeliveryRuleSetView::from_rule_set(&template.policy.bulk),
            updated_by: template.updated_by.clone(),
            updated_at: template.base.updated_at,
        }
    }
}

/// 供给当前配送模板摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferingDeliveryView {
    /// 来源。
    pub source: DeliveryTemplateSource,
    /// 生效模板主键。
    pub template_id: Option<String>,
    /// 生效模板名称。
    pub template_name: Option<String>,
    /// 界面展示名称。
    pub display_name: String,
}

impl OfferingDeliveryView {
    /// 由解析结果生成摘要。
    ///
    /// # 参数
    /// * `resolved` - 供给生效配送政策
    ///
    /// # 返回
    /// 返回摘要视图。
    ///
    /// # 错误
    /// 无。
    pub fn from_resolved(resolved: &ResolvedDelivery) -> Self {
        Self {
            source: resolved.source,
            template_id: resolved.template_id.as_ref().map(ToString::to_string),
            template_name: resolved.template_name.clone(),
            display_name: resolved.display_name(),
        }
    }
}

fn validation(error: Error) -> Error {
    match error {
        Error::Logic(error) => Error::ValidationError(error.to_string()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::delivery_policy::DeliveryTerm;

    use super::{
        AssignDeliveryTemplateRequest, DeliveryRegionRuleInput, DeliveryRuleSetInput,
        SaveDeliveryTemplateRequest,
    };
    use crate::Error;

    fn request(region: &str) -> SaveDeliveryTemplateRequest {
        SaveDeliveryTemplateRequest {
            name: "常温".into(),
            is_default: false,
            dropship: DeliveryRuleSetInput { default_term: DeliveryTerm::FreightIncluded, rules: vec![] },
            bulk: DeliveryRuleSetInput {
                default_term: DeliveryTerm::FreightExtra,
                rules: vec![DeliveryRegionRuleInput {
                    region_code: region.into(),
                    term: DeliveryTerm::FreightIncluded,
                }],
            },
            expected_version: None,
        }
    }

    #[test]
    fn into_content_parses_region_codes() {
        let content = request(" 440000 ").into_content().unwrap();
        assert_eq!(content.bulk.rules[0].region_code.as_str(), "440000");
    }

    #[test]
    fn into_content_rejects_unknown_region_as_validation_error() {
        assert!(matches!(request("449999").into_content(), Err(Error::ValidationError(_))));
    }

    #[test]
    fn assign_request_dedupes_and_limits_offerings() {
        let request = AssignDeliveryTemplateRequest {
            offering_ids: vec![" a ".into(), "a".into(), "b".into(), " ".into()],
            template_id: None,
        };
        assert_eq!(request.normalized_offering_ids().unwrap(), ["a", "b"]);
        let empty = AssignDeliveryTemplateRequest { offering_ids: vec![], template_id: None };
        assert!(empty.normalized_offering_ids().is_err());
        let many = AssignDeliveryTemplateRequest {
            offering_ids: (0..201).map(|index| index.to_string()).collect(),
            template_id: None,
        };
        assert!(many.normalized_offering_ids().is_err());
    }
}
