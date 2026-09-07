//! 结算成本差额缺少权威原成本链时保持原失败关闭。
use crate::entity::cost::CostEntry;
use crate::repository::CostExt;
use crate::{Error, Result};
use erp_core::money::Amount;
use mongodb::Database;
use persistence_core::Executor;
use std::str::FromStr;
/// 财务成本实际消费的差额三元组；不补建不存在的成本来源事实。
#[derive(Debug, Clone, Copy)]
pub struct SettlementCostDeltaFact {
    pub gross: Amount,
    pub net: Amount,
    pub tax: Amount,
}
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}
/// 仅全零三元组返回空计划，其余保持原业务错误；不得伪造 CostEntry。
pub fn build_settlement_cost_delta(delta: &SettlementCostDeltaFact) -> Result<Vec<CostEntry>> {
    if delta.gross == zero_amount() && delta.net == zero_amount() && delta.tax == zero_amount() {
        return Ok(Vec::new());
    }
    Err(Error::BusinessLogicError(
        "ERP_ACCEPTED 成本差额暂缺权威原成本、税率与消费分配链，禁止伪造 CostEntry".to_string(),
    ))
}

/// 逐条复用原成本仓储 entry→allocations 写序；不得另开事务。
pub async fn persist_settlement_costs(
    db: &Database,
    entries: &[CostEntry],
    executor: &mut dyn Executor,
) -> Result<()> {
    for entry in entries {
        db.cost()
            .create_cost_entry_with_allocations(entry, Vec::new(), executor)
            .await?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cost_delta_writer_blocks_nonzero_delta_without_authoritative_lineage() {
        let delta = SettlementCostDeltaFact {
            gross: Amount::from_str("1.00").unwrap(),
            net: Amount::from_str("0.87").unwrap(),
            tax: Amount::from_str("0.13").unwrap(),
        };
        assert!(build_settlement_cost_delta(&delta).is_err());
    }
    #[test]
    fn cost_delta_writer_skips_zero_delta() {
        assert!(build_settlement_cost_delta(&SettlementCostDeltaFact {
            gross: zero_amount(),
            net: zero_amount(),
            tax: zero_amount()
        })
        .unwrap()
        .is_empty());
    }
    #[test]
    fn each_nonzero_component_preserves_original_fail_closed_error() {
        for (gross, net, tax) in [
            ("1.00", "0.00", "0.00"),
            ("0.00", "1.00", "0.00"),
            ("0.00", "0.00", "1.00"),
        ] {
            let error = build_settlement_cost_delta(&SettlementCostDeltaFact {
                gross: Amount::from_str(gross).unwrap(),
                net: Amount::from_str(net).unwrap(),
                tax: Amount::from_str(tax).unwrap(),
            })
            .unwrap_err();
            assert!(
                matches!(error,Error::BusinessLogicError(ref value) if value=="ERP_ACCEPTED 成本差额暂缺权威原成本、税率与消费分配链，禁止伪造 CostEntry")
            );
        }
    }
}
