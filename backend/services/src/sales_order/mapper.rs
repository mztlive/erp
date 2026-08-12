//! DTO ↔ 实体/视图映射：构建稳定明细、工作副本、提交快照与视图转换。

use entities::common::time::Instant;
use entities::ids::{
    SalesOrderId, SalesOrderLineId, SalesOrderSubmissionId, SalesOrderSubmissionLineId,
    SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId,
};
use entities::sales_order::{
    GoodsLineFields, LineType, SalesOrder, SalesOrderLine, SalesOrderLineData, SalesOrderSubmission,
    SalesOrderSubmissionData, SalesOrderSubmissionLine, SalesOrderSubmissionLineData, SalesOrderWorkingCopy,
    SalesOrderWorkingCopyData, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData, VoucherLineDraft,
    WorkingPurpose,
};
use id_generator::next_id;

use super::dto::{
    SalesOrderDraftLineRequest, SalesOrderDraftRequest, SalesOrderWorkingCopyLineView, SubmissionView,
};
use super::pricing::line_totals;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 构建稳定明细行（订单创建时按草稿行号建立）。
///
/// # 参数
/// * `order_id` - 所属销售单
/// * `lines` - 草稿行请求
///
/// # 返回
/// 返回稳定明细行清单（行号升序）。
///
/// # 错误
/// 行号重复时返回错误。
pub(super) fn build_stable_lines(
    order_id: &SalesOrderId,
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderLine>> {
    let mut stable = Vec::with_capacity(lines.len());
    for line in lines {
        if stable
            .iter()
            .any(|existing: &SalesOrderLine| existing.line_no == line.line_no)
        {
            return Err(Error::ValidationError(format!("行号 {} 重复", line.line_no)));
        }
        stable.push(SalesOrderLine::new(
            SalesOrderLineId::new(next_id()),
            order_id.clone(),
            SalesOrderLineData {
                line_no: line.line_no,
            },
        )?);
    }
    Ok(stable)
}

/// 构建工作副本（含行实体）。
///
/// 行金额由实体 `new` 逐行舍入计算，表头金额由服务端汇总已舍入的行金额
/// （§4.2 铁律 2）。
///
/// # 参数
/// * `order` - 所属销售单实体（已建）
/// * `stable_lines` - 稳定明细行（已建，行号与草稿行一一对应）
/// * `draft` - 草稿表头与明细请求
/// * `draft_version` - 初始草稿版本
/// * `actor` - 操作人
///
/// # 返回
/// 返回 `(工作副本实体, 工作副本行清单)`。
///
/// # 错误
/// 表头快照、行字段组或金额三元组校验失败时返回错误。
pub(super) fn build_working_copy(
    order: &SalesOrder,
    stable_lines: &[SalesOrderLine],
    draft: &SalesOrderDraftRequest,
    draft_version: u32,
    actor: &AuditActor,
) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>)> {
    let order_id = SalesOrderId::new(order.base.id.clone());
    let working_copy_id = SalesOrderWorkingCopyId::new(next_id());
    // 行创建数据同时用于：① 头实体 `validate_line_list` 跨行断言；② 行实体落库。
    // 此前传空 Vec 会导致「销售单明细不能为空」——头校验与行集合分离的契约要求
    // 创建数据必须携带完整行摘要（即使行最终分集合存储）。
    let line_datas = build_working_copy_line_datas(stable_lines, &draft.lines)?;
    let lines = materialize_working_copy_lines(&working_copy_id, &line_datas)?;
    let (gross, net, tax) = line_totals(&lines);
    let snapshot = header_snapshot(draft)?;
    let working_copy = SalesOrderWorkingCopy::new(
        working_copy_id,
        SalesOrderWorkingCopyData {
            sales_order_id: order_id,
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version,
            content_hash: draft_hash(&order.base.id, draft_version),
            editor_user_id: draft.editor_user_id.clone(),
            business_type: order.business_type,
            customer_id: order.customer_id.clone(),
            contract_id: order.contract_id.clone(),
            settlement_party_id: order.settlement_party_id.clone(),
            snapshot,
            project_name: draft.project_name.clone(),
            business_remark: draft.business_remark.clone(),
            voucher_category_sku_id: draft.voucher_category_sku_id.clone(),
            voucher_expiry_at: draft
                .voucher_expiry_at
                .map(|secs| Instant::from_unix_secs(secs as i64)),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            lines: line_datas,
        },
        actor.id(),
    )?;
    Ok((working_copy, lines))
}

/// 将草稿行请求映射为工作副本行创建数据（稳定行号 → 稳定明细身份）。
///
/// # 参数
/// * `stable_lines` - 稳定明细行（行号与草稿行一一对应）
/// * `lines` - 草稿行请求
///
/// # 返回
/// 返回工作副本行创建数据清单。
///
/// # 错误
/// 行号无对应稳定明细时返回错误。
fn build_working_copy_line_datas(
    stable_lines: &[SalesOrderLine],
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderWorkingCopyLineData>> {
    let mut datas = Vec::with_capacity(lines.len());
    for line in lines {
        let stable_id = stable_lines
            .iter()
            .find(|stable| stable.line_no == line.line_no)
            .map(|stable| stable.base.id.clone())
            .ok_or_else(|| Error::ValidationError(format!("行号 {} 无对应稳定明细", line.line_no)))?;
        datas.push(SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new(stable_id),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods: line.goods.clone(),
            voucher: line.voucher.clone(),
        });
    }
    Ok(datas)
}

/// 由行创建数据物化工作副本行实体。
///
/// # 参数
/// * `working_copy_id` - 所属工作副本 ID
/// * `line_datas` - 行创建数据
///
/// # 返回
/// 返回工作副本行实体清单（金额由实体逐行舍入计算）。
///
/// # 错误
/// 行字段组与行类型不一致、金额非法时返回错误。
fn materialize_working_copy_lines(
    working_copy_id: &SalesOrderWorkingCopyId,
    line_datas: &[SalesOrderWorkingCopyLineData],
) -> Result<Vec<SalesOrderWorkingCopyLine>> {
    let mut built = Vec::with_capacity(line_datas.len());
    for data in line_datas {
        built.push(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new(next_id()),
            working_copy_id.clone(),
            data.clone(),
        )?);
    }
    Ok(built)
}

/// 构建工作副本行实体（草稿保存路径复用：稳定行 + 草稿请求 → 行实体）。
///
/// # 参数
/// * `_order_id` - 所属销售单（预留，稳定行 ID 由建单实体产生）
/// * `working_copy_id` - 所属工作副本 ID
/// * `stable_lines` - 稳定明细行（行号与草稿行一一对应）
/// * `lines` - 草稿行请求
///
/// # 返回
/// 返回工作副本行清单（金额由实体逐行舍入计算）。
///
/// # 错误
/// 行字段组与行类型不一致、金额非法时返回错误。
pub(super) fn build_working_copy_lines(
    _order_id: &SalesOrderId,
    working_copy_id: &SalesOrderWorkingCopyId,
    stable_lines: &[SalesOrderLine],
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderWorkingCopyLine>> {
    let line_datas = build_working_copy_line_datas(stable_lines, lines)?;
    materialize_working_copy_lines(working_copy_id, &line_datas)
}

/// 构建表头快照入参。
///
/// # 参数
/// * `draft` - 草稿表头请求
///
/// # 返回
/// 返回表头快照入参。
pub(super) fn header_snapshot(
    draft: &SalesOrderDraftRequest,
) -> Result<entities::sales_order::HeaderSnapshotData> {
    Ok(entities::sales_order::HeaderSnapshotData {
        customer_name: draft.customer_name.clone(),
        contract_no: draft.contract_no.clone(),
        settlement_party_name: draft.settlement_party_name.clone(),
        payment_term_code: draft.payment_term_code.clone(),
        payment_term_name: draft.payment_term_name.clone(),
        invoice_type: draft.invoice_type.clone(),
        tax_point: draft.tax_point.clone(),
    })
}

/// 生成草稿内容指纹（服务端确定性派生，供幂等与历史查询）。
///
/// # 参数
/// * `id` - 业务对象 ID
/// * `version` - 草稿版本
///
/// # 返回
/// 返回 128 字符内的内容指纹。
pub(super) fn draft_hash(id: &str, version: u32) -> String {
    format!("draft:{id}:{version}")
}

/// 从工作副本构建提交快照。
///
/// # 参数
/// * `working_copy` - 已迁移到 `Submitted` 的工作副本
/// * `lines` - 工作副本行
/// * `submission_no` - 提交序号
/// * `actor` - 提交人
///
/// # 返回
/// 返回提交快照实体。
///
/// # 错误
/// 提交字段校验失败时返回错误。
pub(super) fn build_submission(
    working_copy: &SalesOrderWorkingCopy,
    lines: &[SalesOrderWorkingCopyLine],
    submission_no: u32,
    actor: &AuditActor,
) -> Result<SalesOrderSubmission> {
    let (gross, net, tax) = line_totals(lines);
    // 提交头 `validate_line_list` 需要行摘要；行实体另集存储，但创建数据必须非空。
    let mut line_datas = Vec::with_capacity(lines.len());
    for line in lines {
        line_datas.push(SalesOrderSubmissionLineData {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods: working_copy_goods(line)?,
            voucher: working_copy_voucher(line)?,
        });
    }
    SalesOrderSubmission::new(
        SalesOrderSubmissionId::new(next_id()),
        SalesOrderSubmissionData {
            sales_order_id: working_copy.sales_order_id.clone(),
            submission_no,
            working_copy_id: working_copy.base.id.clone().into(),
            working_copy_version: working_copy.draft_version,
            business_type: working_copy.business_type,
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: None,
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
                customer_name: working_copy.customer_snapshot.customer_name.clone(),
                contract_no: working_copy
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: working_copy
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: working_copy.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: working_copy.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: working_copy.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: working_copy.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: working_copy.project_name.clone(),
            business_remark: working_copy.business_remark.clone(),
            voucher_category_sku_id: working_copy.voucher_category_sku_id.clone(),
            voucher_expiry_at: working_copy.voucher_expiry_at,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            submitted_at: Instant::now(),
            submitted_by: actor.id().to_string(),
            lines: line_datas,
        },
    )
    .map_err(Error::Logic)
}

/// 从工作副本行构建提交快照明细。
///
/// # 参数
/// * `submission` - 提交快照
/// * `lines` - 工作副本行
///
/// # 返回
/// 返回提交快照明细清单。
///
/// # 错误
/// 行字段组缺失或非法时返回错误。
pub(super) fn build_submission_lines(
    submission: &SalesOrderSubmission,
    lines: &[SalesOrderWorkingCopyLine],
) -> Result<Vec<SalesOrderSubmissionLine>> {
    let mut built = Vec::with_capacity(lines.len());
    for line in lines {
        let goods = working_copy_goods(line)?;
        let voucher = working_copy_voucher(line)?;
        built.push(SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new(next_id()),
            submission.base.id.clone().into(),
            SalesOrderSubmissionLineData {
                sales_order_line_id: line.sales_order_line_id.clone(),
                line_no: line.line_no,
                line_type: line.line_type,
                sales_tax_rate: line.sales_tax_rate,
                item_name_snapshot: line.item_name_snapshot.clone(),
                spec_snapshot: line.spec_snapshot.clone(),
                unit_snapshot: line.unit_snapshot.clone(),
                goods,
                voucher,
            },
        )?);
    }
    Ok(built)
}

/// 从工作副本行还原实物及服务字段组。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；卡券行返回 `None`。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
fn working_copy_goods(line: &SalesOrderWorkingCopyLine) -> Result<Option<GoodsLineFields>> {
    if line.line_type != LineType::GoodsService {
        return Ok(None);
    }
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(Some(GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode,
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    }))
}

/// 从工作副本行还原卡券字段组。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；实物及服务行返回 `None`。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
fn working_copy_voucher(line: &SalesOrderWorkingCopyLine) -> Result<Option<VoucherLineDraft>> {
    if line.line_type != LineType::Voucher {
        return Ok(None);
    }
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    let face_value_total = line
        .face_value_total
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?;
    let transaction_amount = line
        .transaction_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?;
    let gift_amount = line
        .gift_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?;
    Ok(Some(VoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total,
        transaction_amount,
        gift_amount,
        gift_rate: line.gift_rate,
        card_form: line
            .card_form
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?,
    }))
}

/// 构造提交历史视图。
///
/// # 参数
/// * `submission` - 提交快照实体
/// * `lines` - 该提交下的明细行
///
/// # 返回
/// 返回视图。
pub(super) fn submission_view(
    submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
) -> SubmissionView {
    SubmissionView {
        id: submission.base.id,
        submission_no: submission.submission_no,
        status: submission.stable.status,
        business_type: submission.business_type,
        customer_name: submission.customer_snapshot.customer_name.clone(),
        contract_no: submission
            .contract_snapshot
            .as_ref()
            .map(|s| s.contract_no.clone()),
        settlement_party_name: submission
            .settlement_party_snapshot
            .as_ref()
            .map(|s| s.settlement_party_name.clone()),
        payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
        payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
        invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
        tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
        project_name: submission.project_name.clone(),
        business_remark: submission.business_remark.clone(),
        voucher_category_sku_id: submission
            .voucher_category_sku_id
            .as_ref()
            .map(ToString::to_string),
        voucher_expiry_at: submission
            .voucher_expiry_at
            .map(|instant| instant.unix_secs() as u64),
        gross_amount: submission.gross_amount,
        net_amount: submission.net_amount,
        tax_amount: submission.tax_amount,
        submitted_by: submission.submitted_by,
        submitted_at: submission.submitted_at.unix_secs() as u64,
        created_at: submission.base.created_at,
        lines: lines.into_iter().map(submission_line_view).collect(),
    }
}

/// 构造工作副本行视图。
///
/// # 参数
/// * `line` - 工作副本行实体
///
/// # 返回
/// 返回视图。
pub(super) fn working_copy_line_view(line: SalesOrderWorkingCopyLine) -> SalesOrderWorkingCopyLineView {
    SalesOrderWorkingCopyLineView {
        id: line.base.id,
        sales_order_line_id: line.sales_order_line_id.to_string(),
        line_no: line.line_no,
        line_type: line.line_type,
        gross_amount: line.gross_amount,
        net_amount: line.net_amount,
        tax_amount: line.tax_amount,
        sales_tax_rate: line.sales_tax_rate,
        item_name_snapshot: line.item_name_snapshot,
        spec_snapshot: line.spec_snapshot,
        unit_snapshot: line.unit_snapshot,
        sku_id: line.sku_id,
        sku_revision_id: line.sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode: line.fulfillment_mode,
        fulfillment_due_at: line.fulfillment_due_at.map(|instant| instant.unix_secs() as u64),
        quantity: line.quantity,
        base_unit_code: line.base_unit_code,
        unit_price_gross: line.unit_price_gross,
        face_value: line.face_value,
        card_count: line.card_count,
        transaction_amount: line.transaction_amount,
        card_form: line.card_form,
    }
}

/// 构造提交明细行视图（与工作副本行视图同形）。
///
/// # 参数
/// * `line` - 提交行实体
///
/// # 返回
/// 返回视图。
fn submission_line_view(line: SalesOrderSubmissionLine) -> SalesOrderWorkingCopyLineView {
    SalesOrderWorkingCopyLineView {
        id: line.base.id,
        sales_order_line_id: line.sales_order_line_id.to_string(),
        line_no: line.line_no,
        line_type: line.line_type,
        gross_amount: line.gross_amount,
        net_amount: line.net_amount,
        tax_amount: line.tax_amount,
        sales_tax_rate: line.sales_tax_rate,
        item_name_snapshot: line.item_name_snapshot,
        spec_snapshot: line.spec_snapshot,
        unit_snapshot: line.unit_snapshot,
        sku_id: line.sku_id,
        sku_revision_id: line.sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode: line.fulfillment_mode,
        fulfillment_due_at: line.fulfillment_due_at.map(|instant| instant.unix_secs() as u64),
        quantity: line.quantity,
        base_unit_code: line.base_unit_code,
        unit_price_gross: line.unit_price_gross,
        face_value: line.face_value,
        card_count: line.card_count,
        transaction_amount: line.transaction_amount,
        card_form: line.card_form,
    }
}
