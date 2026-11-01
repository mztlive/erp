//! 条码归属占用事实为不同 SKU 的建档事务提供共同唯一写入点。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::non_empty_trimmed;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 同一条码只能由一个稳定 SKU 身份占用，多次修订保留同一占用事实。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SkuBarcodeClaim {
    #[serde(flatten)]
    pub base: BaseModel,
    pub barcode: String,
    pub sku_id: String,
}

impl SkuBarcodeClaim {
    /// 根据实体规范化规则建立唯一条码占用事实。
    /// # 参数
    /// `barcode` 为实际条码原文，`sku_id` 为稳定公司 SKU 身份。
    /// # 返回
    /// 返回以规范条码为稳定主键的占用事实。
    /// # 错误
    /// 条码或 SKU 身份缺失、过长时拒绝。
    pub fn new(barcode: &str, sku_id: &str) -> Result<Self> {
        let barcode = non_empty_trimmed(barcode, "条码不能为空")?;
        let sku_id = non_empty_trimmed(sku_id, "条码占用 SKU 不能为空")?;
        if barcode.chars().count() > 128 || sku_id.chars().count() > 128 {
            return Err(Error::ValidationError("条码或占用 SKU 标识过长".into()));
        }
        Ok(Self { base: BaseModel::new(barcode.clone()), barcode, sku_id })
    }

    /// 既有占用及原正式修订中不得存在其他 SKU 身份。
    /// # 参数
    /// `sku_id` 为本次归属，`revision_owners` 为同事务内读取的条码修订归属。
    /// # 返回
    /// 本 SKU 的重复修订允许继续复用占用事实。
    /// # 错误
    /// 其他占用者或历史歧义归属要求重新核对，不自动合并或猜测。
    pub fn ensure_owner(&self, sku_id: &str, revision_owners: &HashSet<String>) -> Result<()> {
        if self.sku_id != sku_id || revision_owners.iter().any(|owner| owner != sku_id) {
            return Err(Error::ConflictError("条码已被其他 SKU 占用或存在歧义，请明确匹配后重新核对".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barcode_identity_is_shared_across_supplier_commands_and_trimmed() {
        let first = SkuBarcodeClaim::new(" 6901234567890 ", "sku-a").unwrap();
        let second = SkuBarcodeClaim::new("6901234567890", "sku-b").unwrap();
        assert_eq!(first.base.id, second.base.id);
        assert_eq!(first.barcode, second.barcode);
        assert!(first.ensure_owner("sku-a", &HashSet::from(["sku-a".into()])).is_ok());
        assert!(matches!(first.ensure_owner("sku-b", &HashSet::new()), Err(Error::ConflictError(_))));
    }

    #[test]
    fn historical_ambiguity_is_rejected_without_guessing_an_owner() {
        let claim = SkuBarcodeClaim::new("BAR", "sku-a").unwrap();
        let owners = HashSet::from(["sku-a".into(), "sku-b".into()]);
        assert!(matches!(claim.ensure_owner("sku-a", &owners), Err(Error::ConflictError(_))));
        assert!(SkuBarcodeClaim::new("  ", "sku-a").is_err());
        assert!(SkuBarcodeClaim::new("BAR", "  ").is_err());
    }
}
