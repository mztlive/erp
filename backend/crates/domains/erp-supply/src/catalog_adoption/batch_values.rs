//! 批次销售参考价草稿、显式编辑与批量值合并。

use erp_core::money::{Amount, Quantity, UnitPrice};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{AdoptionSalesPrices, BatchPricing, FillPolicy, PriceRule, ValueOrigin};
use crate::{Error, Result};

/// 保存字段值及其来源；明确清空与尚未填写分别保存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchValue<T> {
    pub value: Option<T>,
    pub origin: ValueOrigin,
}

impl<T> Default for BatchValue<T> {
    fn default() -> Self {
        Self { value: None, origin: ValueOrigin::Unset }
    }
}

impl<T: Clone> BatchValue<T> {
    /// 建立来源预填值，缺失时保持未设置。
    /// # 参数
    /// `value` 为经过核验的来源值。
    /// # 返回
    /// 来源值及明确的值来源。
    /// # 错误
    /// 不返回错误。
    pub fn initial(value: Option<T>) -> Self {
        let origin = if value.is_some() { ValueOrigin::Source } else { ValueOrigin::Unset };
        Self { value, origin }
    }

    /// 按覆盖策略合并批量值；空规则不清空已存内容。
    /// # 参数
    /// `value` 为本次批量结果，`policy` 为用户明确选择的填值范围。
    /// # 返回
    /// 返回合并后的字段；人工值与明确清空按策略保护。
    /// # 错误
    /// 不返回错误。
    pub fn apply(&self, value: Option<T>, policy: FillPolicy) -> Self {
        match value {
            Some(value) if policy.allows(self.origin) => {
                Self { value: Some(value), origin: ValueOrigin::Rule }
            },
            _ => self.clone(),
        }
    }

    /// 应用单项显式补丁，值来源由服务端生成。
    /// # 参数
    /// `patch` 区分保持、填写和明确清空。
    /// # 返回
    /// 保持原字段，或标为人工填写、明确清空。
    /// # 错误
    /// 不返回错误；字段业务约束由所属值对象检查。
    pub fn edit(&self, patch: &BatchValuePatch<T>) -> Self {
        match patch {
            BatchValuePatch::Keep => self.clone(),
            BatchValuePatch::Set(value) => Self { value: Some(value.clone()), origin: ValueOrigin::Manual },
            BatchValuePatch::Clear => Self { value: None, origin: ValueOrigin::Cleared },
        }
    }

    fn preserved(&self) -> Self {
        Self { value: self.value.clone(), origin: ValueOrigin::Preserved }
    }

    fn reset_rule(&self) -> Self {
        if self.origin == ValueOrigin::Rule { Self::default() } else { self.clone() }
    }
}

/// 单项编辑只接受明确动作，不接受客户端指定字段来源。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum BatchValuePatch<T> {
    #[default]
    Keep,
    Set(T),
    Clear,
}

/// 单项公司销售参考价编辑；缺失字段保持已保存值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchSalesEdit {
    #[serde(default)]
    pub dropship: BatchValuePatch<Amount>,
    #[serde(default)]
    pub bulk: BatchValuePatch<Amount>,
    #[serde(default)]
    pub minimum_quantity: BatchValuePatch<Quantity>,
    #[serde(default)]
    pub market_price: BatchValuePatch<Amount>,
}

/// 一行公司销售参考价及单项覆盖；采购报价不保存在可编辑字段中。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchSalesValues {
    pub dropship: BatchValue<Amount>,
    pub bulk: BatchValue<Amount>,
    pub minimum_quantity: BatchValue<Quantity>,
    pub market_price: BatchValue<Amount>,
    pub bulk_linked: bool,
}

impl BatchSalesValues {
    /// 建立新规格草稿；仅沿用现行来源市场参考价预填。
    /// # 参数
    /// `market_price` 为已核验的来源市场参考价。
    /// # 返回
    /// 公司销售价及门槛为空、市场参考价带来源标记的草稿。
    /// # 错误
    /// 不返回错误；金额合法性在核对时检查。
    pub fn initial(market_price: Option<Amount>) -> Self {
        Self { market_price: BatchValue::initial(market_price), ..Self::default() }
    }

    /// 以已有公司 SKU 当前修订建立只读原价展示。
    /// # 参数
    /// `prices` 为经当前权限及数据范围核验的公司 SKU 原价格。
    /// # 返回
    /// 每个字段均标为原值保留的草稿，不建立批量同价联动。
    /// # 错误
    /// 不返回错误，不更改已有修订。
    pub fn existing(prices: AdoptionSalesPrices) -> Self {
        Self {
            dropship: BatchValue { value: prices.sales_visible_price_gross, origin: ValueOrigin::Preserved },
            bulk: BatchValue { value: prices.bulk_price_gross, origin: ValueOrigin::Preserved },
            minimum_quantity: BatchValue { value: prices.bulk_min_quantity, origin: ValueOrigin::Preserved },
            market_price: BatchValue { value: prices.market_price, origin: ValueOrigin::Preserved },
            bulk_linked: false,
        }
    }

    /// 使旧报价计算出的规则值失效，保留人工、清空、来源及已有 SKU 原值。
    /// # 参数
    /// 无；由来源变更核对入口调用，随后必须重新应用当前批量规则。
    /// # 返回
    /// 旧规则值恢复为未设置的新草稿，不隐式计算或提交价格。
    /// # 错误
    /// 不返回错误。
    pub fn reset_rule_values(&self) -> Self {
        Self {
            dropship: self.dropship.reset_rule(),
            bulk: self.bulk.reset_rule(),
            minimum_quantity: self.minimum_quantity.reset_rule(),
            market_price: self.market_price.reset_rule(),
            bulk_linked: false,
        }
    }

    /// 只从本次核验报价应用公司售价规则，并保护已有 SKU。
    /// # 参数
    /// `pricing`、`policy` 为本批设置，`source` 为接口采购报价，`existing_sku` 表示保留原价。
    /// # 返回
    /// 全部计算成功后的新草稿；失败不修改原草稿。
    /// # 错误
    /// 规则非法、所需报价缺失、计算溢出或同价联动缺代发价时拒绝。
    pub fn apply(
        &self,
        pricing: &BatchPricing,
        policy: FillPolicy,
        source: Option<UnitPrice>,
        existing_sku: bool,
    ) -> Result<Self> {
        if existing_sku {
            return Ok(self.preserved());
        }
        self.ensure_new_sku_values()?;
        let mut next = self.clone();
        match pricing {
            BatchPricing::DropshipOnly { dropship } => {
                next.apply_dropship(dropship, policy, source)?;
                next.bulk_linked = false;
            },
            BatchPricing::Same { rule, minimum_quantity } => {
                next.apply_dropship(rule, policy, source)?;
                next.apply_linked_bulk(policy)?;
                next.minimum_quantity = next.minimum_quantity.apply(*minimum_quantity, policy);
            },
            BatchPricing::Separate { dropship, bulk, minimum_quantity } => {
                next.apply_dropship(dropship, policy, source)?;
                next.bulk = next.bulk.apply(bulk.calculate(source)?, policy);
                next.minimum_quantity = next.minimum_quantity.apply(*minimum_quantity, policy);
                next.bulk_linked = false;
            },
        }
        next.validate_values()?;
        Ok(next)
    }

    fn apply_dropship(
        &mut self,
        rule: &PriceRule,
        policy: FillPolicy,
        source: Option<UnitPrice>,
    ) -> Result<()> {
        self.dropship = self.dropship.apply(rule.calculate(source)?, policy);
        Ok(())
    }

    fn apply_linked_bulk(&mut self, policy: FillPolicy) -> Result<()> {
        if policy.allows(self.bulk.origin) || self.bulk_linked {
            if self.dropship.value.is_none() && !self.bulk_linked {
                return Ok(());
            }
            let value = self.dropship.value.ok_or_else(|| {
                Error::ValidationError("公司代发价为空，不能使用集采与代发同价，请先填写代发价".into())
            })?;
            self.bulk = BatchValue { value: Some(value), origin: ValueOrigin::Rule };
            self.bulk_linked = true;
        }
        Ok(())
    }

    /// 应用单项人工调整，同价联动使用最终人工代发价。
    /// # 参数
    /// `edit` 为服务端处理的显式字段补丁，`existing_sku` 为已有 SKU 保护标记。
    /// # 返回
    /// 带人工或清空标记的新草稿；单独编辑集采价将解除该行联动。
    /// # 错误
    /// 试图修改已有 SKU、金额负数、门槛非正或清空联动代发价时拒绝。
    pub fn edit(&self, edit: &BatchSalesEdit, existing_sku: bool) -> Result<Self> {
        if (existing_sku || self.has_preserved_values()) && edit.has_changes() {
            return Err(Error::ValidationError("已存在公司 SKU 保留原价，不能在本批修改".into()));
        }
        let mut next = Self {
            dropship: self.dropship.edit(&edit.dropship),
            bulk: self.bulk.edit(&edit.bulk),
            minimum_quantity: self.minimum_quantity.edit(&edit.minimum_quantity),
            market_price: self.market_price.edit(&edit.market_price),
            bulk_linked: self.bulk_linked && matches!(edit.bulk, BatchValuePatch::Keep),
        };
        if next.bulk_linked {
            next.apply_linked_bulk(FillPolicy::default())?;
        }
        next.validate_values()?;
        Ok(next)
    }

    /// 取消公司销售价及门槛的单项覆盖，按最新核验报价恢复批量规则。
    /// # 参数
    /// `pricing` 为当前批量规则，`source` 为最新核验报价，`existing_sku` 保护已有规格。
    /// # 返回
    /// 市场参考价保持不变，其他销售字段按批量规则重新生成。
    /// # 错误
    /// 规则无法计算或同价缺少代发价时拒绝。
    pub fn restore(
        &self,
        pricing: &BatchPricing,
        source: Option<UnitPrice>,
        existing_sku: bool,
    ) -> Result<Self> {
        if existing_sku {
            return Ok(self.preserved());
        }
        Self { market_price: self.market_price.clone(), ..Self::default() }.apply(
            pricing,
            FillPolicy::default(),
            source,
            false,
        )
    }

    /// 转为现有采纳命令的公司销售参考价，不产生供应商集采条款。
    /// # 参数
    /// 无。
    /// # 返回
    /// 只包含公司售价、公司门槛及市场参考价的命令值。
    /// # 错误
    /// 不返回错误；调用方必须先执行本行核对。
    pub fn to_sales_prices(&self) -> AdoptionSalesPrices {
        AdoptionSalesPrices {
            sales_visible_price_gross: self.dropship.value,
            bulk_price_gross: self.bulk.value,
            bulk_min_quantity: self.minimum_quantity.value,
            market_price: self.market_price.value,
        }
    }

    /// 核对报价资格、公司集采配对及需要用户确认的价格提示。
    /// # 参数
    /// `source` 为已核验采购报价；`existing_sku` 为原价保留标记。
    /// # 返回
    /// 本行需要明确确认的提示；已有 SKU 不对原售价产生新提示。
    /// # 错误
    /// 缺来源报价、负报价、非法售价或公司集采价及门槛不成对时拒绝。
    pub fn validate(&self, source: Option<UnitPrice>, existing_sku: bool) -> Result<Vec<BatchPriceWarning>> {
        let source = source
            .ok_or_else(|| Error::ValidationError("暂无接口采购报价，请等待来源补齐后重新核对".into()))?;
        if source.to_decimal().is_sign_negative() {
            return Err(Error::ValidationError("接口采购报价不能为负数，请核实来源报价".into()));
        }
        if existing_sku {
            return Ok(Vec::new());
        }
        self.ensure_new_sku_values()?;
        self.validate_values()?;
        if self.bulk.value.is_some() != self.minimum_quantity.value.is_some() {
            return Err(Error::ValidationError("公司集采参考价与公司集采起订量须同时填写或同时留空".into()));
        }
        Ok(self.warnings(Some(source)))
    }

    /// 给出当前可见报价与售价之间的提示，不隐式改变任何价格。
    /// # 参数
    /// `source` 为当前用户有权读取的采购报价。
    /// # 返回
    /// 低于采购报价与集采高于代发的互相独立提示。
    /// # 错误
    /// 不返回错误；来源报价缺失时仍可提示两种公司售价关系。
    pub fn warnings(&self, source: Option<UnitPrice>) -> Vec<BatchPriceWarning> {
        let mut warnings = Vec::new();
        if let Some(source) = source {
            if self.dropship.value.is_some_and(|price| price.to_decimal() < source.to_decimal()) {
                warnings.push(BatchPriceWarning::DropshipBelowPurchaseQuote);
            }
            if self.bulk.value.is_some_and(|price| price.to_decimal() < source.to_decimal()) {
                warnings.push(BatchPriceWarning::BulkBelowPurchaseQuote);
            }
        }
        if self.bulk.value.zip(self.dropship.value).is_some_and(|(bulk, dropship)| bulk > dropship) {
            warnings.push(BatchPriceWarning::BulkHigherThanDropship);
        }
        warnings
    }

    fn preserved(&self) -> Self {
        Self {
            dropship: self.dropship.preserved(),
            bulk: self.bulk.preserved(),
            minimum_quantity: self.minimum_quantity.preserved(),
            market_price: self.market_price.preserved(),
            bulk_linked: false,
        }
    }

    /// 判断草稿是否包含已有公司 SKU 的只读价格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 任意价格或门槛来自已有 SKU 原值时返回真。
    /// # 错误
    /// 不返回错误；匹配由已有 SKU 变成新建时，调用方须重新初始化草稿。
    pub fn has_preserved_values(&self) -> bool {
        [self.dropship.origin, self.bulk.origin, self.minimum_quantity.origin, self.market_price.origin]
            .contains(&ValueOrigin::Preserved)
    }

    fn ensure_new_sku_values(&self) -> Result<()> {
        if self.has_preserved_values() {
            return Err(Error::ConflictError("公司商品匹配结果已变化，请重新初始化新规格售价后核对".into()));
        }
        Ok(())
    }

    fn validate_values(&self) -> Result<()> {
        if [self.dropship.value, self.bulk.value, self.market_price.value]
            .into_iter()
            .flatten()
            .any(|value| value.to_decimal().is_sign_negative())
        {
            return Err(Error::ValidationError("公司销售参考价及市场参考价不能为负数".into()));
        }
        if self.minimum_quantity.value.is_some_and(|quantity| quantity.to_decimal() <= Decimal::ZERO) {
            return Err(Error::ValidationError("公司集采起订量须大于零，最多六位小数".into()));
        }
        Ok(())
    }
}

impl BatchSalesEdit {
    /// 判断本次是否包含实际修改动作。
    /// # 参数
    /// 无。
    /// # 返回
    /// 任意字段为填写或清空时返回真。
    /// # 错误
    /// 不返回错误。
    pub fn has_changes(&self) -> bool {
        !matches!(self.dropship, BatchValuePatch::Keep)
            || !matches!(self.bulk, BatchValuePatch::Keep)
            || !matches!(self.minimum_quantity, BatchValuePatch::Keep)
            || !matches!(self.market_price, BatchValuePatch::Keep)
    }
}

/// 价格提示与业务错误分离，用户明确确认后可以继续核对。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchPriceWarning {
    DropshipBelowPurchaseQuote,
    BulkBelowPurchaseQuote,
    BulkHigherThanDropship,
}

impl BatchPriceWarning {
    /// 返回价格提示的用户可见说明。
    /// # 参数
    /// 无。
    /// # 返回
    /// 对应当前售价关系的中文提示。
    /// # 错误
    /// 不返回错误。
    pub fn label(self) -> &'static str {
        match self {
            Self::DropshipBelowPurchaseQuote => "公司代发参考价低于接口采购报价，请确认",
            Self::BulkBelowPurchaseQuote => "公司集采参考价低于接口采购报价，请确认",
            Self::BulkHigherThanDropship => "公司集采参考价高于公司代发参考价，请确认",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(value: &str) -> Option<UnitPrice> {
        Some(value.parse().unwrap())
    }

    fn same() -> BatchPricing {
        BatchPricing::Same {
            rule: PriceRule::Percentage("15".parse().unwrap()),
            minimum_quantity: Some("10".parse().unwrap()),
        }
    }

    fn apply_same() -> BatchSalesValues {
        BatchSalesValues::default().apply(&same(), FillPolicy::default(), source("100"), false).unwrap()
    }

    fn manual(dropship: Option<&str>, bulk: Option<&str>) -> BatchSalesEdit {
        BatchSalesEdit {
            dropship: dropship.map(|value| BatchValuePatch::Set(value.parse().unwrap())).unwrap_or_default(),
            bulk: bulk.map(|value| BatchValuePatch::Set(value.parse().unwrap())).unwrap_or_default(),
            ..BatchSalesEdit::default()
        }
    }

    #[test]
    fn initial_market_price_never_becomes_a_company_sales_price() {
        let draft = BatchSalesValues::initial(Some("199.00".parse().unwrap()));
        assert_eq!(draft.market_price.origin, ValueOrigin::Source);
        assert_eq!(draft.market_price.value, Some("199.00".parse().unwrap()));
        assert_eq!(draft.dropship, BatchValue::default());
        assert_eq!(draft.bulk, BatchValue::default());
        assert_eq!(draft.minimum_quantity, BatchValue::default());
        assert!(!draft.bulk_linked);
    }

    #[test]
    fn unset_default_keeps_every_price_unfilled() {
        let draft = BatchSalesValues::default();
        let result = draft.apply(&BatchPricing::default(), FillPolicy::default(), None, false).unwrap();
        assert_eq!(result, draft);
    }

    #[test]
    fn same_price_links_to_final_manual_dropship_and_unlinks_on_bulk_edit() {
        let draft = apply_same();
        assert_eq!(draft.dropship.value, Some("115.00".parse().unwrap()));
        assert_eq!(draft.bulk.value, draft.dropship.value);
        assert!(draft.bulk_linked);
        let manual_drop = draft.edit(&manual(Some("118.00"), None), false).unwrap();
        assert_eq!(manual_drop.dropship.origin, ValueOrigin::Manual);
        assert_eq!(manual_drop.bulk.value, Some("118.00".parse().unwrap()));
        assert_eq!(manual_drop.bulk.origin, ValueOrigin::Rule);
        let exception = manual_drop.edit(&manual(None, Some("115.00")), false).unwrap();
        assert!(!exception.bulk_linked);
        let next = exception.edit(&manual(Some("119.00"), None), false).unwrap();
        assert_eq!(next.bulk.value, Some("115.00".parse().unwrap()));
        assert_eq!(next.bulk.origin, ValueOrigin::Manual);
    }

    #[test]
    fn separate_prices_use_original_quote_and_never_compound() {
        let rules = BatchPricing::Separate {
            dropship: PriceRule::Percentage("15".parse().unwrap()),
            bulk: PriceRule::Percentage("10".parse().unwrap()),
            minimum_quantity: Some("10".parse().unwrap()),
        };
        let policy = FillPolicy { overwrite: true, preserve_manual: true };
        let draft = BatchSalesValues::default().apply(&rules, policy, source("100"), false).unwrap();
        assert_eq!(draft.dropship.value, Some("115.00".parse().unwrap()));
        assert_eq!(draft.bulk.value, Some("110.00".parse().unwrap()));
        assert_eq!(draft.apply(&rules, policy, source("100"), false).unwrap(), draft);
        assert!(!draft.bulk_linked);
    }

    #[test]
    fn fill_empty_protects_source_manual_and_explicitly_cleared_values() {
        for origin in [ValueOrigin::Source, ValueOrigin::Manual, ValueOrigin::Cleared] {
            let original = BatchValue {
                value: (origin != ValueOrigin::Cleared).then(|| "88.00".parse::<Amount>().unwrap()),
                origin,
            };
            assert_eq!(original.apply(Some("99.00".parse().unwrap()), FillPolicy::default()), original);
        }
        let original = BatchValue::<Amount>::default();
        let applied = original.apply(Some("99.00".parse().unwrap()), FillPolicy::default());
        assert_eq!(applied.value, Some("99.00".parse().unwrap()));
        assert_eq!(applied.origin, ValueOrigin::Rule);
    }

    #[test]
    fn overwriting_manual_values_requires_explicit_policy_and_removes_manual_origin() {
        let draft = apply_same().edit(&manual(Some("118.00"), Some("112.00")), false).unwrap();
        let preserve = FillPolicy { overwrite: true, preserve_manual: true };
        let retained = draft.apply(&same(), preserve, source("100"), false).unwrap();
        assert_eq!(retained.dropship.value, Some("118.00".parse().unwrap()));
        assert_eq!(retained.bulk.value, Some("112.00".parse().unwrap()));
        assert!(!retained.bulk_linked);
        let overwrite = FillPolicy { overwrite: true, preserve_manual: false };
        let replaced = draft.apply(&same(), overwrite, source("100"), false).unwrap();
        assert_eq!(replaced.dropship.value, Some("115.00".parse().unwrap()));
        assert_eq!(replaced.bulk.value, Some("115.00".parse().unwrap()));
        assert_eq!(replaced.dropship.origin, ValueOrigin::Rule);
        assert_eq!(replaced.bulk.origin, ValueOrigin::Rule);
        assert!(replaced.bulk_linked);
    }

    #[test]
    fn clear_keeps_distinct_origin_and_breaks_bulk_link() {
        let edit = BatchSalesEdit {
            bulk: BatchValuePatch::Clear,
            minimum_quantity: BatchValuePatch::Clear,
            market_price: BatchValuePatch::Clear,
            ..BatchSalesEdit::default()
        };
        let draft = apply_same().edit(&edit, false).unwrap();
        assert!(draft.bulk.value.is_none());
        assert_eq!(draft.bulk.origin, ValueOrigin::Cleared);
        assert_eq!(draft.minimum_quantity.origin, ValueOrigin::Cleared);
        assert!(!draft.bulk_linked);
        let result = draft.apply(&same(), FillPolicy::default(), source("100"), false).unwrap();
        assert_eq!(result, draft);
        assert!(draft.validate(source("100"), false).is_ok());
    }

    #[test]
    fn linked_dropship_cannot_be_cleared_alone() {
        let original = apply_same();
        let edit = BatchSalesEdit { dropship: BatchValuePatch::Clear, ..BatchSalesEdit::default() };
        assert!(original.edit(&edit, false).is_err());
        assert_eq!(original.dropship.value, Some("115.00".parse().unwrap()));
        assert!(original.bulk_linked);
        let edit = BatchSalesEdit {
            dropship: BatchValuePatch::Clear,
            bulk: BatchValuePatch::Clear,
            minimum_quantity: BatchValuePatch::Clear,
            ..BatchSalesEdit::default()
        };
        let cleared = original.edit(&edit, false).unwrap();
        assert!(cleared.validate(source("100"), false).is_ok());
    }

    #[test]
    fn restore_uses_latest_quote_and_preserves_market_exception() {
        let edit = BatchSalesEdit {
            dropship: BatchValuePatch::Set("118.00".parse().unwrap()),
            bulk: BatchValuePatch::Set("112.00".parse().unwrap()),
            market_price: BatchValuePatch::Clear,
            ..BatchSalesEdit::default()
        };
        let draft = apply_same().edit(&edit, false).unwrap();
        let restored = draft.restore(&same(), source("200"), false).unwrap();
        assert_eq!(restored.dropship.value, Some("230.00".parse().unwrap()));
        assert_eq!(restored.bulk.value, restored.dropship.value);
        assert!(restored.bulk_linked);
        assert_eq!(restored.dropship.origin, ValueOrigin::Rule);
        assert_eq!(restored.market_price.origin, ValueOrigin::Cleared);
    }

    #[test]
    fn existing_sku_retains_every_value_and_disallows_manual_edits() {
        let draft = apply_same();
        let result =
            draft.apply(&same(), FillPolicy { overwrite: true, preserve_manual: false }, None, true).unwrap();
        assert_eq!(result.dropship.value, draft.dropship.value);
        assert_eq!(result.bulk.value, draft.bulk.value);
        assert_eq!(result.minimum_quantity.value, draft.minimum_quantity.value);
        assert_eq!(result.dropship.origin, ValueOrigin::Preserved);
        assert_eq!(result.market_price.origin, ValueOrigin::Preserved);
        assert!(!FillPolicy { overwrite: true, preserve_manual: false }.allows(ValueOrigin::Preserved));
        assert!(result.edit(&manual(Some("1"), None), true).is_err());
        assert!(result.edit(&BatchSalesEdit::default(), true).is_ok());
        assert_eq!(result.validate(source("1000"), true).unwrap(), Vec::new());
    }

    #[test]
    fn fixed_sale_does_not_bypass_missing_purchase_quote_and_zero_remains_valid() {
        let pricing = BatchPricing::DropshipOnly { dropship: PriceRule::Fixed("99.00".parse().unwrap()) };
        let draft = BatchSalesValues::default().apply(&pricing, FillPolicy::default(), None, false).unwrap();
        assert!(draft.validate(None, false).is_err());
        assert!(draft.validate(source("0"), false).is_ok());
        assert!(draft.validate(source("-0.01"), false).is_err());
        assert!(draft.validate(None, true).is_err());
    }

    #[test]
    fn company_bulk_price_and_quantity_must_form_a_positive_pair() {
        let draft = BatchSalesValues::default().edit(&manual(None, Some("0")), false).unwrap();
        assert!(draft.validate(source("0"), false).is_err());
        let quantity = BatchSalesEdit {
            minimum_quantity: BatchValuePatch::Set("0.000001".parse().unwrap()),
            ..BatchSalesEdit::default()
        };
        let complete = draft.edit(&quantity, false).unwrap();
        assert!(complete.validate(source("0"), false).is_ok());
        for value in ["0", "-0.000001"] {
            let edit = BatchSalesEdit {
                minimum_quantity: BatchValuePatch::Set(value.parse().unwrap()),
                ..BatchSalesEdit::default()
            };
            assert!(complete.edit(&edit, false).is_err());
        }
        let quantity_only = BatchSalesValues::default().edit(&quantity, false).unwrap();
        assert!(quantity_only.validate(source("0"), false).is_err());
    }

    #[test]
    fn negative_prices_rejected_for_each_editable_price() {
        for edit in [
            manual(Some("-0.01"), None),
            manual(None, Some("-0.01")),
            BatchSalesEdit {
                market_price: BatchValuePatch::Set("-0.01".parse().unwrap()),
                ..BatchSalesEdit::default()
            },
        ] {
            assert!(BatchSalesValues::default().edit(&edit, false).is_err());
        }
    }

    #[test]
    fn warnings_describe_current_comparison_without_changing_prices() {
        let draft = apply_same().edit(&manual(Some("90"), Some("95")), false).unwrap();
        assert_eq!(
            draft.validate(source("100"), false).unwrap(),
            vec![
                BatchPriceWarning::DropshipBelowPurchaseQuote,
                BatchPriceWarning::BulkBelowPurchaseQuote,
                BatchPriceWarning::BulkHigherThanDropship,
            ]
        );
        assert_eq!(draft.dropship.value, Some("90".parse().unwrap()));
        assert_eq!(draft.bulk.value, Some("95".parse().unwrap()));
        assert_eq!(draft.warnings(None), vec![BatchPriceWarning::BulkHigherThanDropship]);
    }

    #[test]
    fn metadata_serializes_and_patch_rejects_client_origins_and_json_numbers() {
        let draft = apply_same();
        let roundtrip: BatchSalesValues =
            serde_json::from_value(serde_json::to_value(&draft).unwrap()).unwrap();
        assert_eq!(roundtrip, draft);
        let patch: BatchSalesEdit = serde_json::from_str(
            r#"{"dropship":{"action":"SET","value":"118.00"},"bulk":{"action":"CLEAR"}}"#,
        )
        .unwrap();
        assert!(patch.has_changes());
        assert!(matches!(patch.market_price, BatchValuePatch::Keep));
        for json in [
            r#"{"dropship":{"action":"SET","value":118}}"#,
            r#"{"dropship":{"action":"SET","value":"118","origin":"RULE"}}"#,
            r#"{"origin":"RULE"}"#,
            r#"{"dropship":{"action":"SET","value":"1.001"}}"#,
            r#"{"minimum_quantity":{"action":"SET","value":"0.0000001"}}"#,
        ] {
            assert!(serde_json::from_str::<BatchSalesEdit>(json).is_err());
        }
    }

    #[test]
    fn dropship_only_keeps_bulk_until_explicit_clear_and_never_hides_a_write() {
        let draft = apply_same();
        let pricing = BatchPricing::DropshipOnly { dropship: PriceRule::Add("10".parse().unwrap()) };
        let result = draft
            .apply(&pricing, FillPolicy { overwrite: true, preserve_manual: true }, source("100"), false)
            .unwrap();
        assert_eq!(result.dropship.value, Some("110".parse().unwrap()));
        assert_eq!(result.bulk.value, draft.bulk.value);
        assert_eq!(result.minimum_quantity.value, draft.minimum_quantity.value);
        assert!(!result.bulk_linked);
        let prices = result.to_sales_prices();
        assert_eq!(prices.bulk_price_gross, draft.bulk.value);
        assert_eq!(prices.bulk_min_quantity, draft.minimum_quantity.value);
    }

    #[test]
    fn failed_rule_application_leaves_original_draft_unchanged() {
        let draft = apply_same();
        let pricing = BatchPricing::Separate {
            dropship: PriceRule::Fixed("999".parse().unwrap()),
            bulk: PriceRule::Add("1".parse().unwrap()),
            minimum_quantity: Some("10".parse().unwrap()),
        };
        assert!(
            draft
                .apply(&pricing, FillPolicy { overwrite: true, preserve_manual: false }, None, false)
                .is_err()
        );
        assert_eq!(draft.dropship.value, Some("115".parse().unwrap()));
        assert_eq!(draft.bulk.value, draft.dropship.value);
    }

    #[test]
    fn existing_values_show_actual_original_prices_and_cannot_be_refilled() {
        let original = AdoptionSalesPrices {
            sales_visible_price_gross: Some("123.00".parse().unwrap()),
            bulk_price_gross: None,
            bulk_min_quantity: None,
            market_price: Some("199.00".parse().unwrap()),
        };
        let draft = BatchSalesValues::existing(original);
        assert_eq!(draft.dropship.value, Some("123.00".parse().unwrap()));
        assert_eq!(draft.bulk.origin, ValueOrigin::Preserved);
        let result = draft
            .apply(&same(), FillPolicy { overwrite: true, preserve_manual: false }, source("100"), true)
            .unwrap();
        assert_eq!(result, draft);
        assert_eq!(draft.reset_rule_values(), draft);
    }

    #[test]
    fn changed_quote_resets_only_rule_values_before_fill_empty_recalculation() {
        let draft = apply_same().edit(&manual(Some("118.00"), None), false).unwrap();
        let reset = draft.reset_rule_values();
        assert_eq!(reset.dropship.value, Some("118.00".parse().unwrap()));
        assert_eq!(reset.dropship.origin, ValueOrigin::Manual);
        assert_eq!(reset.bulk, BatchValue::default());
        assert_eq!(reset.minimum_quantity, BatchValue::default());
        let result = reset.apply(&same(), FillPolicy::default(), source("200"), false).unwrap();
        assert_eq!(result.bulk.value, Some("118.00".parse().unwrap()));
        let rule_draft = apply_same().reset_rule_values();
        let result = rule_draft.apply(&same(), FillPolicy::default(), source("200"), false).unwrap();
        assert_eq!(result.dropship.value, Some("230.00".parse().unwrap()));
        assert_eq!(result.bulk.value, result.dropship.value);
        let cleared = draft
            .edit(
                &BatchSalesEdit {
                    bulk: BatchValuePatch::Clear,
                    minimum_quantity: BatchValuePatch::Clear,
                    ..BatchSalesEdit::default()
                },
                false,
            )
            .unwrap()
            .reset_rule_values();
        assert_eq!(cleared.bulk.origin, ValueOrigin::Cleared);
        assert_eq!(cleared.minimum_quantity.origin, ValueOrigin::Cleared);
    }

    #[test]
    fn original_company_prices_cannot_be_reused_for_a_newly_classified_sku() {
        let draft = BatchSalesValues::existing(AdoptionSalesPrices {
            sales_visible_price_gross: Some("123".parse().unwrap()),
            ..AdoptionSalesPrices::default()
        });
        assert!(draft.has_preserved_values());
        assert!(draft.apply(&same(), FillPolicy::default(), source("100"), false).is_err());
        assert!(draft.validate(source("100"), false).is_err());
        assert!(draft.edit(&manual(Some("123"), None), false).is_err());
        let new_draft = BatchSalesValues::initial(Some("199".parse().unwrap()));
        assert_eq!(
            new_draft.apply(&same(), FillPolicy::default(), source("100"), false).unwrap().dropship.value,
            Some("115".parse().unwrap())
        );
    }
}
