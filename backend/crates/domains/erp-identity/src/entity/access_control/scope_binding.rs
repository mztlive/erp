//! DataScope v2 的资源动作绑定与目标维度，禁止隐式全资源及身份混用。

use erp_core::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize};

use super::DataScopeType;
use super::scope_target::BoundScopeTarget;

/// 范围目标的身份维度；各维度独立匹配，同名身份也不能混用。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDimension {
    /// 匹配内部组织归属；静态与动态组织目标均使用此维度。
    InternalOrg,
    /// 匹配业务对象的结算主体，目标须显式指定。
    SettlementParty,
    /// 匹配业务对象的仓库，目标须显式指定。
    Warehouse,
}

/// 组织或团队范围取得目标身份的方式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeTargetMode {
    /// 使用配置中明确列出的目标身份。
    Explicit,
    /// 按判断时点的有效成员关系取得本人的内部组织。
    OwnOrg,
    /// 旧角色的管理部门模式，只支持内部组织。
    ManagedOrgs,
}

/// 与范围类型和目标同行保存的 v2 绑定，限定适用资源动作及目标解释方式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeBinding {
    /// 存储协议版本；反序列化时必须提供且等于 2。
    #[serde(deserialize_with = "schema_version")]
    pub schema_version: u8,
    /// 规则适用的单个业务资源标识。
    pub resource: String,
    /// 规则适用的明确动作标识列表，须有 1 至 32 项。
    pub actions: Vec<String>,
    /// 目标身份的匹配维度；动态模式只允许内部组织。
    pub target_dimension: ScopeDimension,
    /// 组织或团队范围必填的目标来源；无需目标的范围留空。
    pub target_mode: Option<ScopeTargetMode>,
    /// 仅内部组织的显式或本人组织模式必填，控制下级组织是否纳入范围。
    pub include_descendants: Option<bool>,
    /// 规则参与资源动作匹配的启用开关。
    pub enabled: bool,
}

impl ScopeBinding {
    /// 校验同行范围类型与目标的 v2 绑定：资源、动作须符合标识格式，动作数须为 1 至 32；具体登记由服务端目录核对。
    /// 无需目标的范围须省略模式、下级标记与目标。组织或团队范围须指定模式：显式模式必须有目标，动态模式只限内部组织且目标须为空。
    /// 下级标记仅在内部组织的显式或本人组织模式下必填，其他模式必须省略。
    pub fn validate(&self, scope_type: DataScopeType, targets: &[String]) -> Result<()> {
        self.target(scope_type, targets).map(|_| ())
    }

    /// 在读取存储或请求数据时先校验版本和资源动作标识，再解析同行的目标；非法绑定或目标返回错误。
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

    /// 筛选适用于当前资源动作的绑定；须为 v2、已启用、资源一致且动作列在绑定中。
    pub fn applies(&self, resource: &str, action: &str) -> bool {
        self.schema_version == 2
            && self.enabled
            && self.resource == resource
            && self.actions.iter().any(|value| value == action)
    }
}

// 检查资源或动作标识：长度为 1 至 128 字节，只允许 ASCII 小写字母、数字及下划线。
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

// 解码绑定时只接受版本 2，使旧版或未知版本在解析目标前失败。
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

    // 构造已启用的 v2 销售单列表绑定，默认使用合法的管理部门目标模式。
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

    // 验证管理部门模式只接受内部组织，并拒绝显式目标和下级标记。
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

    // 验证绑定只匹配启用规则列出的资源动作，跨资源、跨动作及停用均不匹配。
    #[test]
    fn binding_never_grants_other_resources_actions_or_disabled_rules() {
        assert!(binding().applies("sales_order", "list"));
        assert!(!binding().applies("sales_order", "update"));
        assert!(!binding().applies("purchase_order", "list"));
        assert!(!ScopeBinding { enabled: false, ..binding() }.applies("sales_order", "list"));
    }

    // 验证旧协议版本及缺省版本字段都在反序列化阶段被拒绝。
    #[test]
    fn unsupported_or_missing_schema_is_rejected_during_decode() {
        let mut json = serde_json::to_value(binding()).unwrap();
        json["schema_version"] = 1.into();
        assert!(serde_json::from_value::<ScopeBinding>(json.clone()).is_err());
        json.as_object_mut().unwrap().remove("schema_version");
        assert!(serde_json::from_value::<ScopeBinding>(json).is_err());
    }
}
