//! 人员唯一业务范围接口。
use serde::{Deserialize, Serialize};

use crate::access_control::ScopeDimension;
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeTerm};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePersonScopeRequest {
    pub resource: String,
    pub actions: Vec<String>,
    pub terms: Vec<PersonScopeTerm>,
    pub expected_policy_version: u64,
}
#[derive(Serialize)]
pub struct PersonBusinessOption {
    pub resource: String,
    pub actions: Vec<String>,
    pub dimensions: Vec<ScopeDimension>,
}
#[derive(Serialize)]
pub struct PersonScopeView {
    pub items: Vec<PersonDataScope>,
    pub businesses: Vec<PersonBusinessOption>,
    pub policy_version: u64,
}

impl SavePersonScopeRequest {
    /// 规范化输入并检查完整业务维度。
    /// # 参数
    /// * `dimensions` - 消费者要求的维度。
    /// # 返回
    /// 去重后的原子保存命令。
    /// # 错误
    /// 空范围、非法动作、缺维度或非法范围项拒绝。
    pub fn normalized(mut self, dimensions: &[ScopeDimension]) -> crate::Result<Self> {
        self.actions.sort();
        self.actions.dedup();
        if self.actions.is_empty()
            || self.actions.len() > 64
            || self.terms.is_empty()
            || self.terms.len() > 16
        {
            return Err(crate::Error::ValidationError("请选择操作和完整数据范围".into()));
        }
        for term in &mut self.terms {
            if matches!(
                term.scope_type,
                crate::access_control::DataScopeType::SelfOwned
                    | crate::access_control::DataScopeType::Collaborative
            ) && term.target_dimension != ScopeDimension::InternalOrg
            {
                return Err(crate::Error::ValidationError("本人及协作范围只适用于业务负责人维度".into()));
            }
            term.scope_targets.sort();
            term.scope_targets.dedup();
            for action in &self.actions {
                term.rule(&self.resource, action, "validation", true)?;
            }
        }
        let company =
            self.terms.iter().any(|term| term.scope_type == crate::access_control::DataScopeType::Company);
        if !company && dimensions.iter().any(|d| !self.terms.iter().any(|term| term.target_dimension == *d)) {
            return Err(crate::Error::ValidationError("请配置此业务全部必需维度".into()));
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::DataScopeType;
    fn request() -> SavePersonScopeRequest {
        SavePersonScopeRequest {
            resource: "sales_order".into(),
            actions: vec!["detail".into(), "detail".into()],
            terms: vec![PersonScopeTerm {
                scope_type: DataScopeType::SelfOwned,
                target_dimension: ScopeDimension::InternalOrg,
                target_mode: None,
                include_descendants: None,
                scope_targets: vec![],
            }],
            expected_policy_version: 1,
        }
    }
    #[test]
    fn normalized_save_deduplicates_and_rejects_missing_dimensions() {
        assert_eq!(request().normalized(&[ScopeDimension::InternalOrg]).unwrap().actions, vec!["detail"]);
        assert!(request().normalized(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]).is_err());
        let mut missing = request();
        missing.terms.clear();
        assert!(missing.normalized(&[]).is_err());
        let mut wildcard = request();
        wildcard.actions = vec!["*".into()];
        assert!(wildcard.normalized(&[]).is_err());
    }
    #[test]
    fn api_refuses_role_source_and_migration_expression() {
        let value = serde_json::json!({"resource":"sales_order","actions":["detail"],"terms":[],"expected_policy_version":1,"role_id":"sales"});
        assert!(serde_json::from_value::<SavePersonScopeRequest>(value).is_err());
    }
}
