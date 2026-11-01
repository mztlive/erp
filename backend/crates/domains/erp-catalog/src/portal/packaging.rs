//! 供应商明确确认的包装关系与原始报价证据，不执行单位或价格自动换算。

use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};

use super::validation::{bounded, required, validation};
use crate::Result;

/// 供应商自行确认转换后保留的原包装与原报价声明。
///
/// 正式供给条款和数量已由供应商按基础单位重新填写；该声明只保留依据，
/// 原始报价金额的合法性由供给领域校验；本领域保留供应商的确认声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagingInput {
    /// 原包装单位，例如“箱”。
    pub original_unit: String,
    /// 供应商本次填写的正式报价和数量基础单位，例如“瓶”。
    pub base_unit: String,
    /// 每个原包装包含的基础单位数量。
    pub units_per_package: Quantity,
    /// 每个原包装的供应商原始报价，使用十进制字符串传输。
    pub original_unit_price: String,
    /// 供应商显式确认包装关系和其自行填写的转换结果。
    pub conversion_confirmed_by_supplier: bool,
}

impl PackagingInput {
    /// 草稿允许尚未完成的包装声明，只限制文本尺寸。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 原包装单位、基础单位和原报价都未超长时返回空结果。
    ///
    /// # 错误
    /// 任一文本超过长度上限时返回 `ValidationError`。
    pub(super) fn validate_storage(&self) -> Result<()> {
        bounded(&self.original_unit, "原包装单位", 64)?;
        bounded(&self.base_unit, "包装基础单位", 64)?;
        bounded(&self.original_unit_price, "原包装报价", 64)?;
        Ok(())
    }

    /// 提交要求包装依据完整且由供应商明确确认，不执行换算。
    ///
    /// # 参数
    /// * `row_unit` - 本行 SKU 本次填写的单位原文
    ///
    /// # 返回
    /// 包装依据完整、数量为正、基础单位与本行一致且已确认时返回空结果。
    ///
    /// # 错误
    /// 文本超长或为空、每个包装的基础单位数量不大于零、基础单位与本行不一致或供应商未确认时返回 `ValidationError`。
    pub(super) fn validate_submission(&self, row_unit: &str) -> Result<()> {
        self.validate_storage()?;
        required(&self.original_unit, "原包装单位", 64)?;
        required(&self.base_unit, "包装基础单位", 64)?;
        required(&self.original_unit_price, "原包装报价", 64)?;
        let quantity = self.units_per_package.to_decimal();
        if quantity.is_zero() || quantity.is_sign_negative() {
            return Err(validation("每个包装的基础单位数量必须大于零"));
        }
        if self.base_unit != row_unit.trim() {
            return Err(validation("包装基础单位必须与 SKU 本次填写的单位一致"));
        }
        if !self.conversion_confirmed_by_supplier {
            return Err(validation("包装关系及转换结果必须由供应商明确确认后重新提交"));
        }
        Ok(())
    }
}
