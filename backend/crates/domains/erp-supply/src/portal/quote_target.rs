//! 首次报价冻结供应商实际核对的公司 SKU、商品修订和基础单位依据。

use serde::{Deserialize, Serialize};

use super::application::ensure_text;
use crate::{Error, Result};

/// 由定向目录展示并随原报价提交的正式目标版本，不允许审核时补造。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuoteTargetVersion {
    pub sku_version: u64,
    pub sku_revision_id: String,
    pub sku_revision_version: u64,
    pub product_id: String,
    pub product_version: u64,
    pub product_revision_id: String,
    pub product_revision_version: u64,
    pub unit_id: String,
    pub unit_version: u64,
}

impl QuoteTargetVersion {
    /// 比较原报价依据与同一写事务读取的当前正式事实。
    /// # 参数
    /// `current` 为目录拥有领域读取的当前事实，必须先核对启用和引用归属。
    /// # 返回
    /// 所有目标标识和版本相同时返回成功，不改写原稿。
    /// # 错误
    /// 缺失标识、非法版本或任一正式依据变化时要求重新核对提交。
    pub fn ensure_current(&self, current: &Self) -> Result<()> {
        for id in [&self.sku_revision_id, &self.product_id, &self.product_revision_id, &self.unit_id] {
            ensure_text(id, "报价目标依据")?;
        }
        if [
            self.sku_version,
            self.sku_revision_version,
            self.product_version,
            self.product_revision_version,
            self.unit_version,
        ]
        .contains(&0)
        {
            return Err(Error::ValidationError("报价目标版本必须为正数".into()));
        }
        if self != current {
            return Err(Error::ConflictError(
                "公司SKU、商品修订或基础单位已经变化，请重新核对报价并提交".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> QuoteTargetVersion {
        QuoteTargetVersion {
            sku_version: 2,
            sku_revision_id: "sku-revision".into(),
            sku_revision_version: 1,
            product_id: "product".into(),
            product_version: 3,
            product_revision_id: "product-revision".into(),
            product_revision_version: 1,
            unit_id: "unit".into(),
            unit_version: 4,
        }
    }

    #[test]
    fn unchanged_target_accepts_and_each_changed_fact_rejects_without_replacing_original() {
        let original = target();
        original.ensure_current(&target()).unwrap();
        let serialized = serde_json::to_value(&original).unwrap();
        for field in [
            "sku_version",
            "sku_revision_version",
            "product_version",
            "product_revision_version",
            "unit_version",
        ] {
            let mut value = serialized.clone();
            value[field] = serde_json::json!(99);
            let current = serde_json::from_value(value).unwrap();
            assert!(matches!(original.ensure_current(&current), Err(Error::ConflictError(_))));
        }
        for field in ["sku_revision_id", "product_id", "product_revision_id", "unit_id"] {
            let mut value = serialized.clone();
            value[field] = serde_json::json!("changed");
            let current = serde_json::from_value(value).unwrap();
            assert!(matches!(original.ensure_current(&current), Err(Error::ConflictError(_))));
        }
        assert_eq!(serde_json::to_value(&original).unwrap(), serialized);
    }

    #[test]
    fn empty_identity_and_zero_version_cannot_be_a_frozen_basis() {
        let mut invalid = target();
        invalid.sku_revision_id.clear();
        assert!(matches!(invalid.ensure_current(&invalid), Err(Error::ValidationError(_))));
        invalid = target();
        invalid.unit_version = 0;
        assert!(matches!(invalid.ensure_current(&invalid), Err(Error::ValidationError(_))));
    }
}
