//! DataScope v2 的资源动作绑定与目标维度，禁止隐式全资源及身份混用。

use erp_core::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize};

use super::DataScopeType;

/// 各目标集合的身份类型；不同维度不得合并。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDimension {
    InternalOrg,
    SettlementParty,
    Warehouse,
}

/// 组织目标的解析来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    ///
    /// # 错误
    /// 缺失资源动作、未知版本、通配符、动态非组织范围或不一致的目标设置均拒绝。
    pub fn validate(&self, scope_type: DataScopeType, targets: &[String]) -> Result<()> {
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
        if !scope_type.requires_targets() {
            if self.target_mode.is_some() || self.include_descendants.is_some() || !targets.is_empty() {
                return Err(Error::from("公司、本人及协作范围不得携带组织目标设置"));
            }
            return Ok(());
        }
        self.validate_targets(targets)
    }

    /// 判断规则是否适用于当前资源与动作。
    ///
    /// # 返回
    /// 停用规则和其他资源动作不贡献任何范围。
    pub fn applies(&self, resource: &str, action: &str) -> bool {
        self.schema_version == 2
            && self.enabled
            && self.resource == resource
            && self.actions.iter().any(|value| value == action)
    }

    /// 校验组织目标模式，动态目标只能使用内部组织。
    fn validate_targets(&self, targets: &[String]) -> Result<()> {
        let mode = self
            .target_mode
            .ok_or_else(|| Error::from("组织范围必须指定目标模式"))?;
        if mode != ScopeTargetMode::Explicit && self.target_dimension != ScopeDimension::InternalOrg {
            return Err(Error::from("动态范围仅支持内部组织"));
        }
        if (mode == ScopeTargetMode::Explicit) == targets.is_empty() {
            return Err(Error::from("显式模式必须指定目标，动态模式不得混入静态目标"));
        }
        let needs_descendants =
            self.target_dimension == ScopeDimension::InternalOrg && mode != ScopeTargetMode::ManagedOrgs;
        if needs_descendants != self.include_descendants.is_some() {
            return Err(Error::from(
                "内部组织显式或本人组织模式必须明确是否包含下级，其他模式不得设置",
            ));
        }
        Ok(())
    }
}

/// 注册标识只允许小写字母、数字和下划线，禁止通配符及显示名称。
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// 在持久化及请求解码边界拒绝不支持的模式版本。
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
        assert!(ScopeBinding {
            target_dimension: ScopeDimension::Warehouse,
            ..binding()
        }
        .validate(DataScopeType::Team, &[])
        .is_err());
        assert!(ScopeBinding {
            include_descendants: Some(true),
            ..binding()
        }
        .validate(DataScopeType::Team, &[])
        .is_err());
    }

    #[test]
    fn binding_never_grants_other_resources_actions_or_disabled_rules() {
        assert!(binding().applies("sales_order", "list"));
        assert!(!binding().applies("sales_order", "update"));
        assert!(!binding().applies("purchase_order", "list"));
        assert!(!ScopeBinding {
            enabled: false,
            ..binding()
        }
        .applies("sales_order", "list"));
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
