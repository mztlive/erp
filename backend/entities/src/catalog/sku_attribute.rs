//! `sku_attribute` 规格属性字典（数据模型 §6.3，稳定字典）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::stable::StableBase;
use crate::errors::Result;
use crate::ids::SkuAttributeId;
use crate::validation::normalize_required_text;

/// 属性代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 属性名称最大长度。
const NAME_MAX_LEN: usize = 128;

/// 属性值类型（数据模型 §6.3：受控枚举或规范文本）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AttributeValueType {
    /// 受控枚举：取值必须来自 `sku_attribute_value` 字典。
    Enum,
    /// 规范文本：保存规范化文本值。
    Text,
}

impl AttributeValueType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enum => "受控枚举",
            Self::Text => "规范文本",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enum => "enum",
            Self::Text => "text",
        }
    }
}

/// 规格属性创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuAttributeData {
    /// 稳定属性代码（唯一，创建后不可修改）。
    pub attribute_code: String,
    /// 属性名称。
    pub name: String,
    /// 属性值类型。
    pub value_type: AttributeValueType,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 规格属性更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SkuAttributeUpdate {
    /// 属性名称；`None` 表示不修改。
    pub name: Option<String>,
    /// 属性值类型；`None` 表示不修改。
    pub value_type: Option<AttributeValueType>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 规格属性实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SkuAttribute {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// 稳定属性代码（创建后不可修改）。
    pub attribute_code: String,
    /// 属性名称。
    pub name: String,
    /// 属性值类型。
    pub value_type: AttributeValueType,
}

impl PartialEq for SkuAttribute {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.attribute_code == other.attribute_code
            && self.name == other.name
            && self.value_type == other.value_type
    }
}

impl Eq for SkuAttribute {}

impl SkuAttribute {
    /// 创建规格属性。
    ///
    /// 完成 attribute_code/name 的校验与规范化（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SkuAttributeId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的属性实体。
    ///
    /// # 错误
    /// 当 attribute_code/name 为空或超长时返回错误。
    pub fn new(id: SkuAttributeId, data: SkuAttributeData, created_by: impl Into<String>) -> Result<Self> {
        let attribute_code = normalize_required_text(
            data.attribute_code,
            "属性代码不能为空",
            CODE_MAX_LEN,
            "属性代码过长",
        )?;
        let name = normalize_required_text(data.name, "属性名称不能为空", NAME_MAX_LEN, "属性名称过长")?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            attribute_code,
            name,
            value_type: data.value_type,
        })
    }

    /// 更新规格属性。
    ///
    /// 复用 `new` 的校验规则；`attribute_code` 是稳定代码，不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当更新字段校验失败时返回错误。
    pub fn update(&mut self, update: SkuAttributeUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "属性名称不能为空", NAME_MAX_LEN, "属性名称过长")?;
        }
        if let Some(value_type) = update.value_type {
            self.value_type = value_type;
        }
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断属性是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::SkuAttributeId;

    fn data() -> SkuAttributeData {
        SkuAttributeData {
            attribute_code: " SIZE ".to_string(),
            name: " 尺码 ".to_string(),
            value_type: AttributeValueType::Enum,
            status: EnableStatus::Active,
        }
    }

    /// happy path：字段 trim 规范化，值类型与状态落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let attribute = SkuAttribute::new(SkuAttributeId::new("attr-1"), data(), "admin-1").unwrap();

        assert_eq!(attribute.attribute_code, "SIZE");
        assert_eq!(attribute.name, "尺码");
        assert_eq!(attribute.value_type, AttributeValueType::Enum);
        assert!(attribute.is_active());
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_code = SkuAttributeData {
            attribute_code: "  ".to_string(),
            ..data()
        };
        assert!(SkuAttribute::new(SkuAttributeId::new("attr-1"), empty_code, "admin-1").is_err());

        let overlong_name = SkuAttributeData {
            name: "n".repeat(129),
            ..data()
        };
        assert!(SkuAttribute::new(SkuAttributeId::new("attr-1"), overlong_name, "admin-1").is_err());
    }

    /// update 修改名称/值类型/状态并 touch 审计人。
    #[test]
    fn update_applies_fields_and_preserves_code() {
        let mut attribute = SkuAttribute::new(SkuAttributeId::new("attr-1"), data(), "admin-1").unwrap();

        attribute
            .update(
                SkuAttributeUpdate {
                    name: Some(" 颜色 ".to_string()),
                    value_type: Some(AttributeValueType::Text),
                    status: Some(EnableStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(attribute.name, "颜色");
        assert_eq!(attribute.value_type, AttributeValueType::Text);
        assert!(!attribute.is_active());
        assert_eq!(attribute.attribute_code, "SIZE");
        assert_eq!(attribute.stable.updated_by, "admin-2");
    }

    /// 值类型 serde 形态与中文标签。
    #[test]
    fn value_type_exposes_labels_and_codes() {
        assert_eq!(
            serde_json::to_string(&AttributeValueType::Enum).unwrap(),
            "\"enum\""
        );
        assert_eq!(AttributeValueType::Text.label(), "规范文本");
        assert_eq!(AttributeValueType::Enum.as_str(), "enum");
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
