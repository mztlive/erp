//! 本次开票申请唯一销售来源的版本摘要，不读取当前销售草稿。
use erp_core::ids::{SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_finance::entity::receivable::{ReceivableAccount, SalesInvoiceRequest};
use erp_sales::entity::sales_order::{
    LineType, SalesOrder, SalesOrderGoodsServiceLineRevision, SalesOrderRevision, SalesOrderRevisionLine,
    SalesOrderVoucherLineRevision,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalRelatedSalesSnapshot;
use mongodb::Database;
use persistence_core::Executor;

use super::super::brief::{BRIEF_LINE_LIMIT, BriefLine, BriefSection, ObjectBriefSource, push_section};
use super::super::presentation::format_yuan;
use crate::{Error, Result};

/// 验证申请、应收和销售属于同一业务来源，禁止跨主体补取资料。
/// # 参数
/// `request` 为已授权申请，`account` 为其绑定应收。
/// # 返回
/// 申请和应收的全部来源身份一致时成功。
/// # 错误
/// 应收、销售、客户或购方主体不一致时返回冲突。
pub(super) fn validate_binding(request: &SalesInvoiceRequest, account: &ReceivableAccount) -> Result<()> {
    if request.receivable_account_id.as_ref() != account.base.id
        || request.sales_order_id != account.sales_order_id
        || request.customer_id != account.customer_id
        || request.counterparty_party_id != account.counterparty_party_id
    {
        return Err(Error::ConflictError("开票申请与应收来源不一致，请联系管理员核对".into()));
    }
    Ok(())
}

/// 读取本次绑定销售单的生效版本；版本改变由上层明确展示。
/// # 参数
/// 数据库、已授权申请及调用方事务执行器。
/// # 返回
/// 当前销售单、生效版本及有界展示摘要。
/// # 错误
/// 来源缺失、客户或版本归属不一致、明细不完整时拒绝，仓储错误原样返回。
pub(super) async fn load(
    db: &Database,
    request: &SalesInvoiceRequest,
    executor: &mut dyn Executor,
) -> Result<(SalesOrder, SalesOrderRevision, ApprovalRelatedSalesSnapshot)> {
    let order = db
        .sales_orders()
        .find_by_id(request.sales_order_id.as_ref(), executor)
        .await?
        .filter(|row| row.customer_id == request.customer_id)
        .ok_or_else(|| Error::ConflictError("开票申请来源销售单不存在或客户不一致".into()))?;
    let id =
        order.current_revision_id().ok_or_else(|| Error::ConflictError("来源销售单没有生效版本".into()))?;
    let revision = db
        .sales_order_revisions()
        .find_by_id(id.as_ref(), executor)
        .await?
        .filter(|row| row.sales_order_id == request.sales_order_id)
        .ok_or_else(|| Error::ConflictError("来源销售版本不匹配".into()))?;
    let display = display(db, &order, &revision, executor).await?;
    Ok((order, revision, display))
}

async fn display(
    db: &Database,
    order: &SalesOrder,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> Result<ApprovalRelatedSalesSnapshot> {
    let mut lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(&revision.base.id), executor)
        .await?;
    lines.sort_by_key(|line| line.line_no);
    let ids = lines
        .iter()
        .take(BRIEF_LINE_LIMIT)
        .map(|row| SalesOrderRevisionLineId::new(&row.base.id))
        .collect::<Vec<_>>();
    let goods =
        db.sales_order_goods_service_line_revisions().list_by_revision_line_ids(&ids, executor).await?;
    let vouchers = db.sales_order_voucher_line_revisions().list_by_revision_line_ids(&ids, executor).await?;
    let brief = lines
        .iter()
        .take(BRIEF_LINE_LIMIT)
        .map(|line| line_brief(line, &goods, &vouchers))
        .collect::<Result<Vec<_>>>()?;
    Ok(ApprovalRelatedSalesSnapshot {
        document_id: order.base.id.clone(),
        document_no: order.order_no.clone(),
        revision_id: revision.base.id.clone(),
        revision_no: revision.revision.revision_no,
        source: ObjectBriefSource {
            customer: Some(revision.customer_snapshot.customer_name.clone()),
            amount_label: Some(format_yuan(&revision.gross_amount)),
            lines: brief,
            more_count: u32::try_from(lines.len().saturating_sub(BRIEF_LINE_LIMIT))
                .map_err(|_| Error::ConflictError("销售明细数量超限".into()))?,
            extra_sections: sections(revision),
            ..Default::default()
        },
    })
}

fn line_brief(
    line: &SalesOrderRevisionLine,
    goods: &[SalesOrderGoodsServiceLineRevision],
    vouchers: &[SalesOrderVoucherLineRevision],
) -> Result<BriefLine> {
    let quantity = match line.line_type {
        LineType::GoodsService => {
            goods.iter().find(|row| row.revision_line_id.as_ref() == line.base.id).map(|row| {
                format!("{} {} · 含税单价 {}", row.quantity, row.base_unit_code, row.unit_price_gross)
            })
        },
        LineType::Voucher => vouchers
            .iter()
            .find(|row| row.revision_line_id.as_ref() == line.base.id)
            .map(|row| format!("{} 张 · 面值 {}", row.card_count, format_yuan(&row.face_value))),
    }
    .ok_or_else(|| Error::ConflictError("销售版本明细不完整，请核对来源资料".into()))?;
    Ok(BriefLine {
        title: line.item_name_snapshot.clone(),
        quantity: Some(quantity),
        due_label: Some(format!(
            "含税 {} · 税额 {}",
            format_yuan(&line.gross_amount),
            format_yuan(&line.tax_amount)
        )),
    })
}

fn sections(revision: &SalesOrderRevision) -> Vec<BriefSection> {
    let mut sections = Vec::new();
    for (label, value) in [
        (
            "我方开票主体",
            revision
                .settlement_party_snapshot
                .as_ref()
                .map(|row| row.settlement_party_name.as_str())
                .unwrap_or("未保存"),
        ),
        (
            "合同编号",
            revision.contract_snapshot.as_ref().map(|row| row.contract_no.as_str()).unwrap_or("未关联合同"),
        ),
        ("付款条件", revision.payment_term_snapshot.payment_term_name.as_str()),
        ("开票要求", revision.invoice_requirement_snapshot.invoice_type.as_str()),
        ("开票时点", "未单独记录，请核对合同约定"),
    ] {
        push_section(&mut sections, label, Some(value), false);
    }
    push_section(&mut sections, "业务说明", revision.business_remark.as_deref(), false);
    sections
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{SalesOrderLineId, SalesOrderVoucherLineRevisionId};
    use erp_sales::entity::sales_order::{
        CardForm, SalesOrderRevisionLineData, SalesOrderVoucherLineRevisionData,
    };

    use super::*;

    fn line() -> SalesOrderRevisionLine {
        let mut row = SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new("line"),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("revision"),
                sales_order_line_id: SalesOrderLineId::new("stable-line"),
                line_no: 1,
                line_type: LineType::Voucher,
                gross_amount: "180".parse().unwrap(),
                net_amount: "180".parse().unwrap(),
                tax_amount: "0".parse().unwrap(),
                sales_tax_rate: "0".parse().unwrap(),
                item_name_snapshot: "福利卡".into(),
                spec_snapshot: None,
                unit_snapshot: Some("张".into()),
            },
        )
        .unwrap();
        row.base = BaseModel { id: "line".into(), ..BaseModel::fake() };
        row
    }

    #[test]
    fn source_summary_uses_frozen_voucher_count_face_value_and_transaction_amount() {
        let line = line();
        let voucher = SalesOrderVoucherLineRevision::new(
            SalesOrderVoucherLineRevisionId::new("voucher"),
            SalesOrderVoucherLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new("line"),
                face_value: "100".parse().unwrap(),
                card_count: 2,
                unit_price_gross: "90".parse().unwrap(),
                card_form: CardForm::Electronic,
            },
        )
        .unwrap();
        let brief = line_brief(&line, &[], &[voucher]).unwrap();
        assert_eq!(brief.title, "福利卡");
        assert!(brief.quantity.unwrap().contains("2 张"));
        assert!(brief.due_label.unwrap().contains("180"));
    }

    #[test]
    fn missing_or_wrong_subtype_does_not_produce_a_partial_sales_basis() {
        let mut line = line();
        assert!(line_brief(&line, &[], &[]).is_err());
        line.line_type = LineType::GoodsService;
        assert!(line_brief(&line, &[], &[]).is_err());
    }
}
