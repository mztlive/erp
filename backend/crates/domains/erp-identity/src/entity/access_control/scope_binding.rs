//! DataScope v2 的资源动作绑定与目标维度，禁止隐式全资源及身份混用。

use erp_core::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize};

use super::DataScopeType;
use super::scope_target::BoundScopeTarget;

/// 各目标集合的身份类型；不同维度不得合并。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDimension {
    InternalOrg,
    SettlementParty,
    Warehouse,
}

/// 组织目标的解析来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeTargetMode {
    Explicit,
    OwnOrg,
    ManagedOrgs,
}

/// 与 DataScope 同行保存的版本 2 授权绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeBinding {
    #[serde(deserialize_with = "schema_version")]
    pub schema_version: u8,
    pub resource: String,
    pub actions: Vec<String>,
    pub target_dimension: ScopeDimension,
    pub target_mode: Option<ScopeTargetMode>,
    pub include_descendants: Option<bool>,
    pub enabled: bool,
}

impl ScopeBinding {
    /// 校验资源动作及目标形态；适用资源和动作由服务端注册目录进一步校验。
    /// 版本、标识和目标形态一致时无返回值。
    ///
    /// `schema_version` 不是 2、资源或动作不是注册标识、动作集合为空或超过 32 个时拒绝。
    /// 不需要目标的类型若携带模式、下级标记或目标则拒绝。
    /// 组织范围缺少模式、动态模式用在非内部组织、显式与动态的目标携带相反，或下级标记与模式不匹配时拒绝。
    ///
    /// `scope_type`：同行范围类型；`targets`：已保存的范围目标。
    pub fn validate(&self, scope_type: DataScopeType, targets: &[String]) -> Result<()> {
        self.target(scope_type, targets).map(|_| ())
    }

    // 在持久化或请求边界一次性验证绑定并返回内部目标。
    // 规范化后的目标形态。
    //
    // 版本、资源动作或范围字段组合不合法时拒绝。
    //
    // scope_type、targets 为同一条范围规则的类型和目标。
    pub(crate) fn target(&self, scope_type: DataScopeType, targets: &[String]) -> Result<BoundScopeTarget> {
        if self.schema_version != 2
            || !identifier(&self.resource)
            || self.actions.is_empty()
            || self.actions.len() > 32
        {
            return Err(Error::from("必须指定版本 2、业务资源及动作集合"));
        }
        if self.actions.iter().any(|action| !identifier(action)) {
            return Err(Error::from("范围动作必须使用明确的注册标识"));
        }
        BoundScopeTarget::parse(self, scope_type, targets)
    }

    /// 当规则适用于当前资源与动作时返回 `true`。
    /// 版本为 2、规则启用、资源相同且动作在列表中时返回 `true`。停用规则和其他资源动作返回 `false`。
    ///
    /// `resource`：当前业务资源；`action`：当前动作。
    pub fn applies(&self, resource: &str, action: &str) -> bool {
        self.schema_version == 2
            && self.enabled
            && self.resource == resource
            && self.actions.iter().any(|value| value == action)
    }
}

// 校验范围登记的稳定标识，只接受 ASCII 小写字母、数字和下划线；不接受通配符或展示名。
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

// 读取持久化绑定或解码外部请求时，拒绝当前实现不支持的范围模式版本。
fn schema_version<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<u8, D::Error> {
    let value = u8::deserialize(deserializer)?;
    if value != 2 {
        return Err(serde::de::Error::custom("仅支持 DataScope schema_version 2"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ScopeBinding {
        ScopeBinding {
            schema_version: 2,
            resource: "sales_order".into(),
            actions: vec!["list".into()],
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: Some(ScopeTargetMode::ManagedOrgs),
            include_descendants: None,
            enabled: true,
        }
    }

    #[test]
    fn dynamic_modes_cannot_mix_targets_or_other_identity_dimensions() {
        assert!(binding().validate(DataScopeType::Team, &[]).is_ok());
        assert!(binding().validate(DataScopeType::Team, &["x".into()]).is_err());
        assert!(
            ScopeBinding { target_dimension: ScopeDimension::Warehouse, ..binding() }
                .validate(DataScopeType::Team, &[])
                .is_err()
        );
        assert!(
            ScopeBinding { include_descendants: Some(true), ..binding() }
                .validate(DataScopeType::Team, &[])
                .is_err()
        );
    }

    #[test]
    fn binding_never_grants_other_resources_actions_or_disabled_rules() {
        assert!(binding().applies("sales_order", "list"));
        assert!(!binding().applies("sales_order", "update"));
        assert!(!binding().applies("purchase_order", "list"));
        assert!(!ScopeBinding { enabled: false, ..binding() }.applies("sales_order", "list"));
    }

    #[test]
    fn unsupported_or_missing_schema_is_rejected_during_decode() {
        let mut json = serde_json::to_value(binding()).unwrap();
        json["schema_version"] = 1.into();
        assert!(serde_json::from_value::<ScopeBinding>(json.clone()).is_err());
        json.as_object_mut().unwrap().remove("schema_version");
        assert!(serde_json::from_value::<ScopeBinding>(json).is_err());
    }
}
