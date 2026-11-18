//! 已采集结算流水与既有来源批次在同一事务关联；仍由既有结算审核决定入账。
use std::collections::{BTreeMap, BTreeSet};

use erp_supply::after_sales::{AfterSalesExt, SettlementEvidence, validate_settlement_source};
use erp_supply::api_order::ApiOrderExt;
use erp_supply::entity::supplier_settlement::SupplierSettlementSourceEvidence;
use erp_supply::ports::connector::settlement::SettlementMovement;
use erp_supply::repository::SupplierFulfillmentExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 在结算调用方事务内关联已采集的原来源流水，不创建新业务事实。
///
/// # 参数
/// `db` 为同一数据库；`source` 为已验证来源批次；`executor` 为结算入口执行器。
///
/// # 返回
/// 原流水与本批次的精确关联已保存。
///
/// # 错误
/// 身份、数量、金额、原流水或版本不一致时拒绝，后续结算写入必须停止。
pub async fn link(
    db: &Database,
    source: &SupplierSettlementSourceEvidence,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut ids = BTreeMap::new();
    for line in &source.lines {
        for reference in &line.evidence_reference_ids {
            if let Some(id) = reference.strip_prefix("supplier-api-settlement:") {
                ids.insert(id.to_string(), ());
            }
        }
    }
    if ids.len() > 1000 {
        return Err(Error::ValidationError("结算来源引用超过上限".into()));
    }

    let mut rows = Vec::new();
    for id in ids.keys() {
        let row = db
            .api_settlement_evidence()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原 API 结算来源流水不存在".into()))?;
        let order = db
            .supplier_fulfillment_orders()
            .find_by_id(&row.evidence.value.order.merchant_order_no, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算来源尚未匹配原履约订单".into()))?;
        let execution = db.supplier_api_order_executions().find_by_id(&order.base.id, executor).await?;
        let external = &row.evidence.value.order.external_order_no;
        let known = order.external_order_no.as_ref() == Some(external)
            || execution
                .as_ref()
                .and_then(|e| e.order_tree.as_ref())
                .is_some_and(|tree| tree.nodes.iter().any(|node| &node.external_order_no == external));
        if !known
            || order.connection_id.as_ref() != row.connection_id
            || order.supplier_id.as_ref() != row.supplier_id
        {
            return Err(Error::ConflictError("结算来源的订单或技术账号不匹配".into()));
        }

        validate_refund_reference(db, source, &row, executor).await?;
        rows.push(row);
    }
    validate_settlement_source(source, &rows)?;

    for mut row in rows {
        if row.source_evidence_id.as_deref() == Some(&source.base.id) {
            continue;
        }
        row.source_evidence_id = Some(source.base.id.clone());
        db.api_settlement_evidence().update(&mut row, executor).await?;
    }
    Ok(())
}

async fn validate_refund_reference(
    db: &Database,
    source: &SupplierSettlementSourceEvidence,
    row: &SettlementEvidence,
    executor: &mut dyn Executor,
) -> Result<()> {
    let SettlementMovement::Refund { external_refund_no, amount } = &row.evidence.value.movement else {
        return Ok(());
    };
    let ids: BTreeSet<_> = source
        .lines
        .iter()
        .filter(|line| {
            line.supplier_fulfillment_order_id.as_ref() == row.evidence.value.order.merchant_order_no
        })
        .flat_map(|line| &line.evidence_reference_ids)
        .filter_map(|reference| {
            reference.strip_prefix("supplier-refund://").and_then(|tail| tail.split('/').next())
        })
        .collect();

    for id in ids {
        if let Some(fact) = db.supplier_refund_facts().find_by_id(id, executor).await?
            && fact.connection_id.as_ref() == row.connection_id
            && fact.external_refund_no == *external_refund_no
            && fact.refund_amount == *amount
            && fact.supplier_fulfillment_order_id.as_ref() == row.evidence.value.order.merchant_order_no
        {
            return Ok(());
        }
    }
    Err(Error::BusinessLogicError("结算退款流水尚未对应正式退款事实，请先完成原退款核对与分配".into()))
}
