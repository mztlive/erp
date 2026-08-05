//! 供应商商品库域内共享枚举与值类型（数据模型 §6.14）。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

/// 属性名最大长度。
const ATTRIBUTE_NAME_MAX_LEN: usize = 128;
/// 属性值最大长度。
const ATTRIBUTE_VALUE_MAX_LEN: usize = 512;

/// 来源类型（§6.14：Excel、API、手工录入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CatalogSourceType {
    /// Excel 导入。
    Excel,
    /// API 同步。
    Api,
    /// 手工录入。
    Manual,
}

impl CatalogSourceType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Excel => "Excel",
            Self::Api => "API",
            Self::Manual => "手工",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Excel => "EXCEL",
            Self::Api => "API",
            Self::Manual => "MANUAL",
        }
    }
}

/// 供应商目录条目状态（§6.14：正常、停止供应、异常）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CatalogItemStatus {
    /// 正常。
    Active,
    /// 停止供应。
    Stopped,
    /// 异常。
    Exception,
}

impl CatalogItemStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "正常",
            Self::Stopped => "停止供应",
            Self::Exception => "异常",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Stopped => "STOPPED",
            Self::Exception => "EXCEPTION",
        }
    }
}

/// 来源规格属性（键值对，可规范化去重；§6.14 结构化描述属性）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAttribute {
    /// 属性名。
    pub attribute_name: String,
    /// 属性值。
    pub attribute_value: String,
}

impl SourceAttribute {
    /// 创建来源属性。
    ///
    /// 校验并规范化属性名与属性值（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `attribute_name` - 属性名
    /// * `attribute_value` - 属性值
    ///
    /// # 返回
    /// 返回属性实例。
    ///
    /// # 错误
    /// 属性名/值为空或超长时返回错误。
    pub fn new(attribute_name: String, attribute_value: String) -> Result<Self> {
        let attribute_name = normalize_required_text(
            attribute_name,
            "属性名不能为空",
            ATTRIBUTE_NAME_MAX_LEN,
            "属性名过长",
        )?;
        let attribute_value = normalize_required_text(
            attribute_value,
            "属性值不能为空",
            ATTRIBUTE_VALUE_MAX_LEN,
            "属性值过长",
        )?;
        Ok(Self {
            attribute_name,
            attribute_value,
        })
    }
}

/// 规范化来源属性列表（去空白、丢弃空条目、同名去重保留首个、数量上限）。
///
/// # 参数
/// * `attributes` - 原始属性列表
/// * `max_count` - 允许的最大属性条数
///
/// # 返回
/// 返回规范化后的属性列表。
///
/// # 错误
/// 属性条目超过上限或单个属性名/值非法时返回错误。
pub fn normalize_attributes(
    attributes: Vec<SourceAttribute>,
    max_count: usize,
) -> Result<Vec<SourceAttribute>> {
    if attributes.len() > max_count {
        return Err(Error::from(format!("来源属性最多 {max_count} 条")));
    }
    let mut normalized: Vec<SourceAttribute> = Vec::with_capacity(attributes.len());
    for attribute in attributes {
        let attribute = SourceAttribute::new(attribute.attribute_name, attribute.attribute_value)?;
        if !normalized
            .iter()
            .any(|item| item.attribute_name == attribute.attribute_name)
        {
            normalized.push(attribute);
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{normalize_attributes, CatalogItemStatus, CatalogSourceType, SourceAttribute};

    #[test]
    fn source_type_and_item_status_expose_labels_and_codes() {
        assert_eq!(CatalogSourceType::Excel.label(), "Excel");
        assert_eq!(CatalogSourceType::Api.as_str(), "API");
        assert_eq!(CatalogSourceType::Manual.as_str(), "MANUAL");
        assert_eq!(CatalogItemStatus::Stopped.label(), "停止供应");
        assert_eq!(CatalogItemStatus::Exception.as_str(), "EXCEPTION");
        assert_eq!(serde_json::to_string(&CatalogSourceType::Api).unwrap(), "\"API\"");
        assert_eq!(
            serde_json::to_string(&CatalogItemStatus::Exception).unwrap(),
            "\"EXCEPTION\""
        );
    }

    #[test]
    fn attribute_normalizes_and_dedups_by_name() {
        let attributes = vec![
            SourceAttribute::new(" 容量 ".to_string(), " 500g ".to_string()).unwrap(),
            SourceAttribute::new("容量".to_string(), "1000g".to_string()).unwrap(),
            SourceAttribute::new("保质期".to_string(), "12个月".to_string()).unwrap(),
        ];
        let normalized = normalize_attributes(attributes, 10).unwrap();
        assert_eq!(normalized.len(), 2, "同名去重保留首个");
        assert_eq!(normalized[0].attribute_name, "容量");
        assert_eq!(normalized[0].attribute_value, "500g");
        assert_eq!(normalized[1].attribute_name, "保质期");
    }

    #[test]
    fn attribute_rejects_blank_and_overlong() {
        assert!(SourceAttribute::new("  ".to_string(), "v".to_string()).is_err());
        assert!(SourceAttribute::new("name".to_string(), " ".to_string()).is_err());
        assert!(SourceAttribute::new("n".repeat(129), "v".to_string()).is_err());

        let many = (0..11)
            .map(|i| SourceAttribute::new(format!("k{i}"), "v".to_string()).unwrap())
            .collect();
        assert!(normalize_attributes(many, 10).is_err());
    }
}
