//! 来源采购限制与按量报价的适用范围；缺失值保留未知，不补造供应商事实。

use erp_core::common::time::Instant;
use erp_core::money::{Quantity, Rate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 目录报价取得时的真实请求范围，与公司销售定价相互独立。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceQuoteContext {
    pub quantity: Quantity,
    pub observed_at: Instant,
    /// 已按来源实际单位转换后的税率；未取得时为空。
    pub tax_rate: Option<Rate>,
    /// 税率来源原文；不推导为统一税率。
    pub tax_rate_raw: Option<String>,
    /// 地址相关报价须绑定已确认地址的摘要，不保存明文地址。
    pub address_fingerprint: Option<String>,
}

impl SourceQuoteContext {
    /// 校验报价的真实数量及来源证据边界。
    /// # 参数
    /// 无；读取当前报价上下文。
    /// # 返回
    /// 数量为正、税率及证据长度合法时成功。
    /// # 错误
    /// 非法数量、税率或不完整地址摘要时拒绝。
    pub fn validate(&self) -> Result<()> {
        if self.quantity.to_decimal() <= Decimal::ZERO
            || self
                .tax_rate
                .is_some_and(|rate| rate.to_decimal() < Decimal::ZERO || rate.to_decimal() > Decimal::ONE)
            || self.tax_rate_raw.as_ref().is_some_and(|raw| raw.trim().is_empty() || raw.len() > 128)
            || self
                .address_fingerprint
                .as_ref()
                .is_some_and(|value| value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(Error::ValidationError("供应商报价数量、税率或适用地址证据无效".into()));
        }
        Ok(())
    }

    /// 判断来源报价是否属于本次真实数量和地址范围。
    /// # 参数
    /// `quantity` 为订购数量，`address_fingerprint` 为本次已确认地址摘要。
    /// # 返回
    /// 真实数量相同且来源地址限制满足时返回真；否则必须重新询价。
    /// # 错误
    /// 不返回错误；不会把目录数量外推到其他采购数量。
    pub fn applies_to(&self, quantity: Quantity, address_fingerprint: Option<&str>) -> bool {
        self.quantity == quantity
            && self.address_fingerprint.as_deref().is_none_or(|scope| Some(scope) == address_fingerprint)
    }
}

/// 需要专门履约支持的来源条件，不得静默降级为普通快递。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseRestriction {
    VirtualProduct,
    Installation,
    Appointment,
    GiftSelection,
    ExtendedWarranty,
    PersonalizedPrice,
    OtherRequiredProcess { code: String },
}

/// 来源明确报告的购买限制；空字段表示未取得，不代表默认一件或任意数量。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePurchaseTerms {
    pub minimum_quantity: Option<Quantity>,
    pub maximum_quantity: Option<Quantity>,
    pub quantity_multiple: Option<Quantity>,
    #[serde(default)]
    pub restrictions: Vec<PurchaseRestriction>,
    pub quote_context: Option<SourceQuoteContext>,
}

impl SourcePurchaseTerms {
    /// 取得用于版本比较的来源条件，排除本地重复采样时间。
    /// # 参数
    /// 无；读取当前来源条款。
    /// # 返回
    /// 保留数量、税率与地址范围，将报价观察时间归一为固定值。
    /// # 错误
    /// 不返回错误；原始证据中的观察时间保持不变。
    pub fn version_content(&self) -> Self {
        let mut content = self.clone();
        if let Some(context) = &mut content.quote_context {
            context.observed_at = Instant::from_unix_secs(0);
        }
        content
    }

    /// 验证来源约束，不接受非正起购量、倍数或倒置区间。
    /// # 参数
    /// 无；读取当前来源条款。
    /// # 返回
    /// 已报告维度均合法时成功。
    /// # 错误
    /// 数量范围、重复限制、来源代码或报价上下文无效时拒绝。
    pub fn validate(&self) -> Result<()> {
        if [self.minimum_quantity, self.maximum_quantity, self.quantity_multiple]
            .into_iter()
            .flatten()
            .any(|quantity| quantity.to_decimal() <= Decimal::ZERO)
            || matches!((self.minimum_quantity, self.maximum_quantity), (Some(min), Some(max)) if min > max)
            || self.restrictions.len() > 32
        {
            return Err(Error::ValidationError("供应商起购、限购数量或购买倍数无效".into()));
        }
        let mut restrictions = self.restrictions.clone();
        restrictions.sort();
        restrictions.dedup();
        if restrictions.len() != self.restrictions.len() || restrictions.iter().any(|restriction| {
            matches!(restriction, PurchaseRestriction::OtherRequiredProcess { code } if code.trim().is_empty() || code.len() > 128)
        }) {
            return Err(Error::ValidationError("供应商附加履约限制无效".into()));
        }
        if let Some(context) = &self.quote_context {
            context.validate()?;
        }
        Ok(())
    }

    /// 按本次真实订购数量核验来源数量限制。
    /// # 参数
    /// `quantity` 为本执行组实际订购的基础单位数量。
    /// # 返回
    /// 正数量同时满足已报告的起购、限购和整倍数时成功。
    /// # 错误
    /// 来源条款无效、数量不满足或计算超限时拒绝。
    pub fn ensure_quantity(&self, quantity: Quantity) -> Result<()> {
        self.validate()?;
        let value = quantity.to_decimal();
        let invalid_multiple = self.quantity_multiple.is_some_and(|multiple| {
            value.checked_rem(multiple.to_decimal()).is_none_or(|remainder| !remainder.is_zero())
        });
        if value <= Decimal::ZERO
            || self.minimum_quantity.is_some_and(|minimum| quantity < minimum)
            || self.maximum_quantity.is_some_and(|maximum| quantity > maximum)
            || invalid_multiple
        {
            return Err(Error::BusinessLogicError("采购数量不满足供应商起购、限购或整倍数要求".into()));
        }
        Ok(())
    }

    /// 核验当前系统可执行普通实物商品自动履约。
    /// # 参数
    /// 无；读取供应商要求的附加流程。
    /// # 返回
    /// 无附加履约要求时成功。
    /// # 错误
    /// 尚未支持的附加服务或个性资格价格存在时阻断自动发单。
    pub fn ensure_automatic_order(&self) -> Result<()> {
        self.validate()?;
        if !self.restrictions.is_empty() {
            return Err(Error::BusinessLogicError(
                "该商品需要尚未支持的附加履约或价格资格，请人工处理".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_quantity_must_satisfy_minimum_maximum_and_exact_multiple() {
        let terms = SourcePurchaseTerms {
            minimum_quantity: Some("2.5".parse().unwrap()),
            maximum_quantity: Some("10".parse().unwrap()),
            quantity_multiple: Some("1.25".parse().unwrap()),
            ..Default::default()
        };
        for quantity in ["2.5", "3.75", "10"] {
            terms.ensure_quantity(quantity.parse().unwrap()).unwrap();
        }
        for quantity in ["0", "-1", "1.25", "3", "11.25"] {
            assert!(terms.ensure_quantity(quantity.parse().unwrap()).is_err());
        }
        let invalid = SourcePurchaseTerms { quantity_multiple: Some("0".parse().unwrap()), ..terms };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn quote_scope_never_promises_price_for_a_different_quantity_or_address() {
        let context = SourceQuoteContext {
            quantity: "1".parse().unwrap(),
            observed_at: Instant::from_unix_secs(100),
            tax_rate: Some("0.09".parse().unwrap()),
            tax_rate_raw: Some("9".into()),
            address_fingerprint: Some("a".repeat(64)),
        };
        context.validate().unwrap();
        assert!(context.applies_to("1".parse().unwrap(), Some(&"a".repeat(64))));
        assert!(!context.applies_to("100".parse().unwrap(), Some(&"a".repeat(64))));
        assert!(!context.applies_to("1".parse().unwrap(), None));
        let terms = SourcePurchaseTerms { quote_context: Some(context), ..Default::default() };
        let restored: SourcePurchaseTerms =
            serde_json::from_str(&serde_json::to_string(&terms).unwrap()).unwrap();
        assert_eq!(restored, terms);
        assert!(SourcePurchaseTerms::default().minimum_quantity.is_none());
    }

    #[test]
    fn required_service_is_preserved_and_blocks_automatic_order() {
        let terms = SourcePurchaseTerms {
            restrictions: vec![PurchaseRestriction::Installation],
            ..Default::default()
        };
        terms.validate().unwrap();
        assert!(terms.ensure_automatic_order().is_err());
        SourcePurchaseTerms::default().ensure_automatic_order().unwrap();
    }
}
