//! 结算单域加载、身份一致性与稳定摘要。
use std::str::FromStr;

use erp_core::money::Amount;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, statement_digest_parts,
};
use crate::repository::SupplierSettlementExt;
use crate::{Error, Result};
pub async fn load_statement_items(
    db: &Database,
    statement_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementItem>> {
    db.supplier_settlement_items().list_by_statement(statement_id, executor).await.map_err(Into::into)
}

pub async fn load_statement_differences(
    db: &Database,
    items: &[SupplierSettlementItem],
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementDifference>> {
    let item_ids = items
        .iter()
        .map(|item| erp_core::ids::SupplierSettlementItemId::new(item.base.id.as_str()))
        .collect::<Vec<_>>();
    db.supplier_settlement_differences()
        .list_by_statement_item_ids(&item_ids, executor)
        .await
        .map_err(Into::into)
}

/// 校验路径身份与命令载荷身份一致。
pub fn ensure_same_id(path_id: &str, command_id: &str, object_name: &str) -> Result<()> {
    if path_id != command_id {
        return Err(Error::ValidationError(format!("{object_name}路径ID与命令载荷不一致")));
    }
    Ok(())
}

/// 对字段逐项加入长度前缀后计算稳定摘要，复用结算主题摘要口径。
pub fn digest_parts(parts: &[String]) -> String {
    statement_digest_parts(parts)
}

/// 返回零金额（表头金额累加起点）。
pub fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 固定的结算复核刷新截止政策。
pub const REVIEW_CUTOFF_POLICY_ID: &str = "supplier-settlement-review-cutoff";
/// 固定的结算复核刷新截止政策版本。
pub const REVIEW_CUTOFF_POLICY_VERSION: &str = "1";
