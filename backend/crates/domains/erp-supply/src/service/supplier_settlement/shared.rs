//! 结算单域加载、身份一致性与稳定摘要。
use std::str::FromStr;

use erp_core::money::Amount;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, statement_digest_parts,
};
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 按结算单加载全部冻结明细。
/// 返回该结算单的明细；没有明细时为空向量。
/// 仓储读取失败时向上传播错误。
pub async fn load_statement_items(
    db: &Database,
    statement_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementItem>> {
    db.supplier_settlement_items().list_by_statement(statement_id, executor).await.map_err(Into::into)
}

/// 按明细身份加载关联的正式差异。
/// 返回这些明细上的差异；没有差异时为空向量。
/// 仓储读取失败时向上传播错误。
/// `items`：已加载的结算明细；只用其身份。
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

/// 核对路径身份与命令载荷身份一致。
/// 两个身份相同时无返回值。
/// 身份不同时返回 `ValidationError`。
pub fn ensure_same_id(path_id: &str, command_id: &str, object_name: &str) -> Result<()> {
    if path_id != command_id {
        return Err(Error::ValidationError(format!(
            "当前选择的{object_name}与提交的单据不一致，请返回列表重新选择"
        )));
    }
    Ok(())
}

/// 对字段逐项加入长度前缀后计算稳定摘要，复用结算主题摘要口径。
/// 返回稳定摘要。
pub fn digest_parts(parts: &[String]) -> String {
    statement_digest_parts(parts)
}

/// 返回零金额（表头金额累加起点）。
/// 返回 `0.00`。
///
/// # Panics
/// `Amount::from_str("0.00")` 失败时 panic。零是合法金额，此分支不应发生。
pub fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 固定的结算复核刷新截止政策。
pub const REVIEW_CUTOFF_POLICY_ID: &str = "supplier-settlement-review-cutoff";
/// 固定的结算复核刷新截止政策版本。
pub const REVIEW_CUTOFF_POLICY_VERSION: &str = "1";
