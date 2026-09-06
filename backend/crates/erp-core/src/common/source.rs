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
    /// 返回用于展示的中文标签。
    ///
    /// # 返回
    /// 返回中文标签字符串。
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

    /// 全部变体的 Display 中文标签与 serde snake_case 形态。
    #[test]
    fn label_and_serde_shape() {
        let cases = [
            (SourceType::Erp, "ERP 系统", "\"erp\""),
            (
                SourceType::SupplierCallback,
                "供应商回调",
                "\"supplier_callback\"",
            ),
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
