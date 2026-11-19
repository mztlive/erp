//! `permission`：配置化权限定义目录（数据模型 §5.1 / §4.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::Result;
use erp_core::ids::PermissionId;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use serde::{Deserialize, Serialize};

use crate::Permission;

/// 权限名称最大长度。
const NAME_MAX_LEN: usize = 64;
/// 权限描述最大长度。
const DESCRIPTION_MAX_LEN: usize = 256;

/// 登记权限目录项时提交的资源、动作及展示资料，交由实体统一校验。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionData {
    /// 权限面向的资源代码，如 `sales_order`。
    pub resource: String,
    /// 权限允许执行的动作代码，如 `approve`。
    pub action: String,
    /// 管理端显示的必填权限名称。
    pub name: String,
    /// 供管理员理解权限用途的可选说明，纯空白视为缺省。
    pub description: Option<String>,
    /// 创建时确定的内建身份；内建定义受删除保护，身份在通用更新中保持不变。
    pub system: bool,
}

impl PermissionData {
    /// 用资源（如 `sales_order`）、动作（如 `approve`）和名称组装待校验数据，
    /// 默认登记为无说明的自定义权限。
    pub fn new(resource: impl Into<String>, action: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            action: action.into(),
            name: name.into(),
            description: None,
            system: false,
        }
    }

    /// 在创建输入中附上用途说明，交由实体构造时规范化。
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 为待登记定义指定内建身份，供后续删除校验使用。
    pub fn with_system(mut self, system: bool) -> Self {
        self.system = system;
        self
    }
}

/// 修改权限目录时提交的局部更新，仅包含可变的展示资料和启停状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PermissionUpdate {
    /// 新的展示名称；`None` 保留原名称。
    pub name: Option<String>,
    /// 新的用途说明；`None` 保留原说明，提交纯空白文本可清空说明。
    pub description: Option<String>,
    /// 新的目录停用状态；`None` 保留当前状态。
    pub disabled: Option<bool>,
}

/// 管理端维护的权限目录定义（数据模型 §5.1），以规范化的 `resource:action` 标识权限。
///
/// 目录记录定义及启停状态，角色与权限的授权绑定继续由 Casbin 规则管理。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PermissionDefinition {
    /// 扁平存储的定义主键、乐观锁版本与时间戳。
    #[serde(flatten)]
    pub base: BaseModel,
    // 已校验的权限身份；存储时拆为历史文档沿用的 `resource` 和 `action` 字段。
    #[serde(flatten, with = "permission_parts")]
    permission: Permission,
    /// 已规范化的权限名称，供管理端展示。
    pub name: String,
    /// 已规范化的可选用途说明，供权限配置时参考。
    pub description: Option<String>,
    /// 是否为系统内建定义；为真时禁止删除，通用更新保持该身份。
    pub system: bool,
    /// 目录项是否停用，供管理端筛选和展示。
    pub disabled: bool,
}

impl PermissionDefinition {
    /// 借用已校验的权限身份，供调用方读取资源和动作。
    pub fn permission(&self) -> &Permission {
        &self.permission
    }

    /// 用调用方分配的 `id` 登记初始启用的权限定义。`Permission::parse` 裁剪组合串首尾空白、
    /// 转为 ASCII 小写，校验单个冒号、字符白名单及每段 128 字节上限。
    /// 权限身份非法、裁剪后的名称为空或超过 64 字符、说明超过 256 字符时返回错误；空白说明视为缺省。
    pub fn new(id: PermissionId, data: PermissionData) -> Result<Self> {
        let parsed = Permission::parse(format!("{}:{}", data.resource, data.action))?;
        let name = normalize_required_text(data.name, "权限名称不能为空", NAME_MAX_LEN, "权限名称过长")?;
        let description = normalize_optional_text(data.description, "权限描述", DESCRIPTION_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            permission: parsed,
            name,
            description,
            system: data.system,
            disabled: false,
        })
    }

    /// 按名称、说明、启停状态的顺序应用已提交字段，保留资源动作和内建身份。
    /// 文本沿用创建时的规范化和校验；出错时此前已赋值的字段保留在内存中，调用方须放弃保存。
    pub fn update(&mut self, update: PermissionUpdate) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "权限名称不能为空", NAME_MAX_LEN, "权限名称过长")?;
        }
        if let Some(description) = update.description {
            self.description = normalize_optional_text(Some(description), "权限描述", DESCRIPTION_MAX_LEN)?;
        }
        if let Some(disabled) = update.disabled {
            self.disabled = disabled;
        }
        Ok(())
    }

    /// 删除前检查内建保护：自定义定义通过，系统内建定义返回「系统权限不能删除」。
    pub fn ensure_deletable(&self) -> Result<()> {
        if self.system {
            return Err(erp_core::Error::from("系统权限不能删除"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::PermissionId;

    use super::{PermissionData, PermissionDefinition, PermissionUpdate};

    // 提供带大写和首尾空白的销售单审批定义，用来验证保存时的规范化。
    fn data() -> PermissionData {
        PermissionData::new(" SalesOrder", "APPROVE", " 销售单审批 ").with_description(" 审批销售单 ")
    }

    // 确认权限写为两个小写顶层字段，正常文档可往返，非法动作在解码时被拒绝。
    #[test]
    fn definition_preserves_document_shape_and_validates_decoded_permission() {
        let value = PermissionDefinition::new(PermissionId::new("p"), data()).unwrap();
        let mut json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["resource"], "salesorder");
        assert_eq!(json["action"], "approve");
        assert!(json.get("permission").is_none());
        assert_eq!(serde_json::from_value::<PermissionDefinition>(json.clone()).unwrap(), value);

        json["action"] = "invalid:action".into();
        assert!(serde_json::from_value::<PermissionDefinition>(json).is_err());
    }

    // 确认新定义规范化资源、动作与展示文本，并默认启用。
    #[test]
    fn new_normalizes_resource_action_and_text() {
        let permission = PermissionDefinition::new(PermissionId::new("perm-1"), data()).unwrap();
        assert_eq!(permission.permission().resource(), "salesorder");
        assert_eq!(permission.permission().action(), "approve");
        assert_eq!(permission.name, "销售单审批");
        assert_eq!(permission.description.as_deref(), Some("审批销售单"));
        assert!(!permission.disabled);
    }

    // 确认动作带额外冒号时，拼成的权限身份无法通过校验。
    #[test]
    fn new_rejects_malformed_resource_action() {
        let payload = PermissionData { action: "approve:extra".to_string(), ..data() };
        assert!(PermissionDefinition::new(PermissionId::new("perm-1"), payload).is_err());
    }

    // 确认新定义拒绝纯空白名称和超过 64 字符的名称。
    #[test]
    fn new_rejects_blank_and_overlong_name() {
        let blank = PermissionData { name: "  ".to_string(), ..data() };
        assert!(PermissionDefinition::new(PermissionId::new("perm-1"), blank).is_err());

        let overlong = PermissionData { name: "名".repeat(65), ..data() };
        assert!(PermissionDefinition::new(PermissionId::new("perm-2"), overlong).is_err());
    }

    // 确认更新可改名和停用，保留资源动作，并拒绝空白的新名称。
    #[test]
    fn update_applies_name_and_disabled_only() {
        let mut permission = PermissionDefinition::new(PermissionId::new("perm-1"), data()).unwrap();
        permission
            .update(PermissionUpdate {
                name: Some(" 新名称 ".to_string()),
                description: None,
                disabled: Some(true),
            })
            .unwrap();
        assert_eq!(permission.name, "新名称");
        assert!(permission.disabled);
        assert_eq!(permission.permission().resource(), "salesorder");
        assert_eq!(permission.permission().action(), "approve");

        assert!(
            permission
                .update(PermissionUpdate { name: Some("  ".to_string()), ..Default::default() })
                .is_err()
        );
    }

    // 确认删除保护拦截内建定义并允许自定义定义通过。
    #[test]
    fn system_permission_is_not_deletable() {
        let system =
            PermissionDefinition::new(PermissionId::new("perm-2"), data().with_system(true)).unwrap();
        assert!(system.ensure_deletable().is_err());
        let custom = PermissionDefinition::new(PermissionId::new("perm-3"), data()).unwrap();
        assert!(custom.ensure_deletable().is_ok());
    }

    // 确认创建输入保留给定的资源、动作和说明，并默认为自定义定义。
    #[test]
    fn permission_data_constructor_sets_required_fields() {
        let data = PermissionData::new("sales_order", "approve", "销售单审批").with_description("审批销售单");
        assert_eq!(data.resource, "sales_order");
        assert_eq!(data.action, "approve");
        assert_eq!(data.description.as_deref(), Some("审批销售单"));
        assert!(!data.system);
    }
}

/// 保持既有 Mongo resource/action 字段，同时在解码边界建立权限不变式。
mod permission_parts {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::Permission;

    // 用于序列化和反序列化的扁平字段桥接，保持历史 MongoDB 文档形状。
    #[derive(Serialize, Deserialize)]
    struct Parts {
        // 文档中的资源代码，回读时与动作一起校验。
        resource: String,
        // 文档中的动作代码，写入时取自已校验的权限值。
        action: String,
    }

    /// 写入时将已校验的权限值拆成 `resource` 和 `action`，沿用文档的顶层字段布局。
    pub(super) fn serialize<S: Serializer>(
        permission: &Permission,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Parts { resource: permission.resource().into(), action: permission.action().into() }
            .serialize(serializer)
    }
    /// 回读时用两个字段重建并校验权限值，字段缺失或权限编码非法时返回解码错误。
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Permission, D::Error> {
        let parts = Parts::deserialize(deserializer)?;
        Permission::parse(format!("{}:{}", parts.resource, parts.action)).map_err(serde::de::Error::custom)
    }
}
