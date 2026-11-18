//! `SourceType` 来源类型枚举。
//!
//! 对应数据模型 4.3「来源类型」：ERP、供应商回调、人工导入。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 事实来源类型。
///
/// serde 形态为 `snake_case`（`erp`/`supplier_callback`/`manual_import`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// ERP 系统自身产生。
    Erp,
    /// 供应商系统回调。
    SupplierCallback,
    /// 人工导入或录入。
    ManualImport,
}

impl SourceType {
    /// 为事实来源提供中文展示名；不要将标签代替持久化或接口使用的固定代码。
    pub fn label(self) -> &'static str {
        match self {
            Self::Erp => "ERP 系统",
            Self::SupplierCallback => "供应商回调",
            Self::ManualImport => "人工导入",
        }
    }
}

impl fmt::Display for SourceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 同时核对来源的中文标签和稳定序列化代码，防止调整展示文案时改掉存储格式。
    #[test]
    fn label_and_serde_shape() {
        let cases = [
            (SourceType::Erp, "ERP 系统", "\"erp\""),
            (SourceType::SupplierCallback, "供应商回调", "\"supplier_callback\""),
            (SourceType::ManualImport, "人工导入", "\"manual_import\""),
        ];

        for (value, label, json) in cases {
            assert_eq!(value.to_string(), label);
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            let back: SourceType = serde_json::from_str(json).unwrap();
            assert_eq!(back, value);
        }
    }
}
