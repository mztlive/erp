//! 库存调整提交与历史回放的余额版本、身份及维度校验。

use std::collections::{HashMap, HashSet};

use super::{StockAdjustment, StockAdjustmentLine};
use crate::entity::inventory::StockBalance;
use crate::{Error, Result};

/// 一次调用方数据库快照已读取的余额事实；按主键查找但按命令输入顺序判定。
pub struct StockAdjustmentBalanceVersions<'a> {
    balances: HashMap<&'a str, &'a StockBalance>,
}

impl<'a> StockAdjustmentBalanceVersions<'a> {
    /// 将同一执行器已读到的未删除库存余额按主键索引，供本次调整逐项核对；不再次查询或执行授权。
    pub fn new(balances: &'a [StockBalance]) -> Self {
        Self { balances: balances.iter().map(|balance| (balance.base.id.as_str(), balance)).collect() }
    }

    /// 提交调整前按原命令顺序核对余额主键是否重复、记录是否存在、版本是否相等及仓库/SKU 是否匹配，再检查是否覆盖全部调整 SKU。
    /// 按该顺序返回首个输入错误，不让后续重复遮盖较早的缺失或版本冲突。
    pub fn validate(
        &self,
        adjustment: &StockAdjustment,
        lines: &[StockAdjustmentLine],
        expected: &[(&str, u64)],
    ) -> Result<()> {
        let required_skus = lines.iter().map(|line| line.sku_id.as_ref()).collect::<HashSet<_>>();
        let mut balance_ids = HashSet::with_capacity(expected.len());
        let mut covered_skus = HashSet::with_capacity(expected.len());

        for &(id, version) in expected {
            if !balance_ids.insert(id) {
                return Err(Error::ValidationError("库存余额版本行不得重复".to_string()));
            }
            let balance =
                self.balances.get(id).ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
            if !balance.matches_version(version) {
                return Err(Error::ConflictError("库存余额已变化，请刷新后重试".to_string()));
            }
            if balance.warehouse_id != adjustment.warehouse_id
                || !required_skus.contains(balance.sku_id.as_ref())
            {
                return Err(Error::ValidationError("库存余额与调整单维度不一致".to_string()));
            }
            covered_skus.insert(balance.sku_id.as_ref());
        }

        if required_skus.iter().any(|sku| !covered_skus.contains(sku)) {
            return Err(Error::ValidationError("提交缺少调整明细对应的库存余额版本".to_string()));
        }
        Ok(())
    }

    /// 恢复历史弱回执时，除使用原提交的余额主键和版本核对最终保存的明细，还要求每个调整 SKU 恰有一条余额要求。
    /// 缺失、重复、维度或版本不符均返回 `false`。
    pub fn matches_legacy_result(
        &self,
        adjustment: &StockAdjustment,
        lines: &[StockAdjustmentLine],
        expected: &[(&str, u64)],
    ) -> bool {
        self.validate(adjustment, lines, expected).is_ok()
            && expected.len() == lines.iter().map(|line| line.sku_id.as_ref()).collect::<HashSet<_>>().len()
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SkuId, StockAdjustmentId, StockAdjustmentLineId, StockBalanceId, WarehouseId};

    use super::*;
    use crate::{
        AdjustmentReasonType, MovementDirection, StockAdjustmentData, StockAdjustmentLineData,
        StockBalanceData,
    };

    // 构造具有一个 SKU 明细的合法调整单。
    fn adjustment() -> (StockAdjustment, Vec<StockAdjustmentLine>) {
        let adjustment = StockAdjustment::new(
            StockAdjustmentId::new("adjustment"),
            StockAdjustmentData {
                adjustment_no: "ADJ-1".into(),
                warehouse_id: WarehouseId::new("warehouse"),
                reason_type: AdjustmentReasonType::StockGain,
                prepared_by: "actor".into(),
                note: None,
                occurred_at: None,
            },
            "actor",
        )
        .unwrap();
        let line = StockAdjustmentLine::new_for_reason(
            StockAdjustmentLineId::new("line"),
            adjustment.reason_type,
            StockAdjustmentLineData {
                stock_adjustment_id: StockAdjustmentId::new("adjustment"),
                sku_id: SkuId::new("sku"),
                quantity: "1".parse().unwrap(),
                direction: MovementDirection::Increase,
            },
        )
        .unwrap();
        (adjustment, vec![line])
    }

    // 构造固定版本的合法余额事实。
    fn balance(id: &str) -> StockBalance {
        let mut balance = StockBalance::new(
            StockBalanceId::new(id),
            StockBalanceData {
                warehouse_id: WarehouseId::new("warehouse"),
                sku_id: SkuId::new("sku"),
                on_hand_quantity: "10".parse().unwrap(),
                reserved_quantity: "0".parse().unwrap(),
                available_quantity: "10".parse().unwrap(),
                last_movement_id: None,
            },
        )
        .unwrap();
        balance.base.version = 3;
        balance
    }

    // 验证完整匹配成立；空要求只能覆盖空明细。
    #[test]
    fn validates_complete_versions_and_empty_boundaries() {
        let (adjustment, lines) = adjustment();
        let balances = [balance("balance")];
        let facts = StockAdjustmentBalanceVersions::new(&balances);
        assert!(facts.validate(&adjustment, &lines, &[("balance", 3)]).is_ok());
        assert!(facts.matches_legacy_result(&adjustment, &lines, &[("balance", 3)]));
        assert!(facts.validate(&adjustment, &[], &[]).is_ok());
        assert!(
            matches!(facts.validate(&adjustment, &lines, &[]), Err(Error::ValidationError(message)) if message == "提交缺少调整明细对应的库存余额版本")
        );
    }

    // 验证后续重复不能盖过前一行缺失或版本冲突。
    #[test]
    fn retains_first_input_failure_before_later_duplicate() {
        let (adjustment, lines) = adjustment();
        let balances = [balance("balance")];
        let facts = StockAdjustmentBalanceVersions::new(&balances);
        assert!(
            matches!(facts.validate(&adjustment, &lines, &[("missing", 3), ("missing", 3)]), Err(Error::NotFound(message)) if message == "库存余额不存在")
        );
        assert!(
            matches!(facts.validate(&adjustment, &lines, &[("balance", 2), ("balance", 2)]), Err(Error::ConflictError(message)) if message == "库存余额已变化，请刷新后重试")
        );
        assert!(
            matches!(facts.validate(&adjustment, &lines, &[("balance", 3), ("balance", 2)]), Err(Error::ValidationError(message)) if message == "库存余额版本行不得重复")
        );
    }

    // 验证同一行先检查版本再检查维度；错误维度仍拒绝。
    #[test]
    fn retains_version_before_dimension_failure() {
        let (adjustment, lines) = adjustment();
        let mut wrong = balance("balance");
        wrong.warehouse_id = WarehouseId::new("other");
        let balances = [wrong];
        let facts = StockAdjustmentBalanceVersions::new(&balances);
        assert!(matches!(
            facts.validate(&adjustment, &lines, &[("balance", 2)]),
            Err(Error::ConflictError(_))
        ));
        assert!(
            matches!(facts.validate(&adjustment, &lines, &[("balance", 3)]), Err(Error::ValidationError(message)) if message == "库存余额与调整单维度不一致")
        );
        assert!(!facts.matches_legacy_result(&adjustment, &lines, &[("balance", 3)]));
    }

    // 验证正式提交覆盖维度即可，历史弱收据还要求每个 SKU 恰有一行余额。
    #[test]
    fn legacy_result_retains_one_to_one_sku_requirement() {
        let (adjustment, lines) = adjustment();
        let balances = [balance("balance-a"), balance("balance-b")];
        let facts = StockAdjustmentBalanceVersions::new(&balances);
        let expected = [("balance-a", 3), ("balance-b", 3)];
        assert!(facts.validate(&adjustment, &lines, &expected).is_ok());
        assert!(!facts.matches_legacy_result(&adjustment, &lines, &expected));
    }
}
