//! DTO ↔ 实体/视图映射：构建稳定明细、工作副本、提交快照与视图转换。

use entities::common::time::Instant;
use entities::ids::{
    SalesOrderId, SalesOrderLineId, SalesOrderSubmissionId, SalesOrderSubmissionLineId,
    SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId,
};
use entities::sales_order::{
    SalesContentHash, SalesOrder, SalesOrderLine, SalesOrderLineData, SalesOrderRevision,
    SalesOrderRevisionLine, SalesOrderSubmission, SalesOrderSubmissionData, SalesOrderSubmissionLine,
    SalesOrderSubmissionLineData, SalesOrderWorkingCopy, SalesOrderWorkingCopyData,
    SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData, WorkingPurpose,
};
use id_generator::next_id;

use super::dto::{
    RevisionLineView, RevisionView, SalesOrderDraftLineRequest, SalesOrderDraftRequest,
    SalesOrderWorkingCopyLineView, SubmissionView,
};
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
    let (gross, net, tax) = SalesOrderWorkingCopyLine::amount_totals(&lines);
    let snapshot = header_snapshot(draft)?;
    let working_copy = SalesOrderWorkingCopy::new(
        working_copy_id,
        SalesOrderWorkingCopyData {
            sales_order_id: order_id,
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version,
            content_hash: SalesContentHash::draft(&order.base.id, draft_version)?.into_wire(),
            editor_user_id: draft.editor_user_id.clone(),
            business_type: order.business_type,
            customer_id: order.customer_id.clone(),
            contract_id: order.contract_id.clone(),
            contract_revision_id: draft.requested_contract_revision_id.clone(),
            settlement_party_id: order.settlement_party_id.clone(),
            snapshot,
            project_name: draft.project_name.clone(),
            business_remark: draft.business_remark.clone(),
            voucher_category_sku_id: draft.voucher_category_sku_id.clone(),
            voucher_expiry_at: draft
                .voucher_expiry_at
                .map(|secs| Instant::from_unix_secs(secs as i64)),
            target_mall_id: draft.target_mall_id.clone(),
            receivable_due_date: draft.receivable_due_date,
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

/// 从工作副本构建提交快照。
///
/// 字段、快照、金额和字段组复制由实体工厂完成；本方法只注入 ID、提交时间与提交人。
///
/// # 参数
/// * `working_copy` - 已迁移到 `Submitted` 的工作副本
/// * `lines` - 工作副本行
/// * `submission_no` - 提交序号
/// * `actor` - 提交人
/// * `customer_external_identity` - 服务端解析的商城客户身份
/// * `voucher_category_external_identity` - 服务端解析的商城卡券类目身份
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
    customer_external_identity: Option<&str>,
    voucher_category_external_identity: Option<&str>,
) -> Result<SalesOrderSubmission> {
    let data = SalesOrderSubmissionData::from_working_copy(
        working_copy,
        lines,
        submission_no,
        Instant::now(),
        actor.id(),
        customer_external_identity,
        voucher_category_external_identity,
    )?;
    SalesOrderSubmission::new(SalesOrderSubmissionId::new(next_id()), data).map_err(Error::Logic)
}

/// 从工作副本行构建提交快照明细。
///
/// 行字段组复制由实体工厂完成；本方法只注入提交行 ID。
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
        built.push(SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new(next_id()),
            submission.base.id.clone().into(),
            SalesOrderSubmissionLineData::from_working_copy_line(line)?,
        )?);
    }
    Ok(built)
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
        contract_revision_id: submission.contract_revision_id.as_ref().map(ToString::to_string),
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
        target_mall_id: submission.target_mall_id.as_ref().map(ToString::to_string),
        customer_external_identity: submission.customer_external_identity.clone(),
        voucher_category_external_identity: submission.voucher_category_external_identity.clone(),
        receivable_due_date: submission.receivable_due_date,
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
        service_region: line.service_region,
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

/// 构造销售版本历史视图（含当时表头快照与公共行摘要）。
///
/// # 参数
/// * `revision` - 正式销售版本实体
/// * `lines` - 该版本下的公共行；调用方须已按行号升序
/// * `previous_revision_no` - 前一生效版本号；无法在本单版本列表解析时为空
///
/// # 返回
/// 返回详情「版本」分区使用的视图。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 快照字段只读正式版本内联值，不得用当前客户/合同主数据回填。
pub(super) fn revision_view(
    revision: SalesOrderRevision,
    lines: Vec<SalesOrderRevisionLine>,
    previous_revision_no: Option<u32>,
) -> RevisionView {
    RevisionView {
        id: revision.base.id,
        revision_no: revision.revision.revision_no,
        revision_source: revision.revision_source,
        content_hash: revision.content_hash,
        customer_name: revision.customer_snapshot.customer_name,
        contract_no: revision.contract_snapshot.map(|snapshot| snapshot.contract_no),
        contract_revision_id: revision.contract_revision_id.as_ref().map(ToString::to_string),
        settlement_party_name: revision
            .settlement_party_snapshot
            .map(|snapshot| snapshot.settlement_party_name),
        payment_term_code: revision.payment_term_snapshot.payment_term_code,
        payment_term_name: revision.payment_term_snapshot.payment_term_name,
        invoice_type: revision.invoice_requirement_snapshot.invoice_type,
        tax_point: revision.invoice_requirement_snapshot.tax_point,
        project_name: revision.project_name,
        business_remark: revision.business_remark,
        previous_revision_id: revision.previous_revision_id.as_ref().map(ToString::to_string),
        previous_revision_no,
        gross_amount: revision.gross_amount,
        net_amount: revision.net_amount,
        tax_amount: revision.tax_amount,
        effective_at: revision.effective_at.unix_secs() as u64,
        created_at: revision.base.created_at,
        line_summary: revision_line_summary(&lines),
        lines: lines.into_iter().map(revision_line_view).collect(),
    }
}

/// 把公共行版本收成详情页一行摘要。
///
/// # 参数
/// * `line` - 公共行版本实体
///
/// # 返回
/// 返回行号、名称、规格、单位与含税金额。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不展开实物/卡券子类型字段，版本列表只展示公共行快照。
fn revision_line_view(line: SalesOrderRevisionLine) -> RevisionLineView {
    RevisionLineView {
        line_no: line.line_no,
        item_name: line.item_name_snapshot,
        spec: line.spec_snapshot,
        unit: line.unit_snapshot,
        gross_amount: line.gross_amount,
    }
}

/// 把版本公共行收成一条可读摘要。
///
/// # 参数
/// * `lines` - 已按行号排序的公共行
///
/// # 返回
/// 无行时返回空串；有行时返回「名称、名称 共 N 项」。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 名称顺序与传入行顺序一致，调用方须先按行号排序。
fn revision_line_summary(lines: &[SalesOrderRevisionLine]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let names = lines
        .iter()
        .map(|line| line.item_name_snapshot.as_str())
        .collect::<Vec<_>>();
    format!("{} 共 {} 项", names.join("、"), lines.len())
}

/// 构造提交明细行视图（与工作副本行视图同形）。
///
/// # 参数
/// * `line` - 提交行实体
///
/// # 返回
/// 返回视图。
///
/// # 错误
/// 无。
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
        service_region: line.service_region,
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::Instant;
    use entities::ids::{
        ContractRevisionId, PartyRevisionId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId,
    };
    use entities::money::{Amount, Rate};
    use entities::sales_order::{
        HeaderSnapshotData, LineType, RevisionSource, SalesOrderRevision, SalesOrderRevisionData,
        SalesOrderRevisionLine, SalesOrderRevisionLineData,
    };

    use super::{revision_line_summary, revision_view};

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn header_data() -> SalesOrderRevisionData {
        SalesOrderRevisionData {
            sales_order_id: SalesOrderId::new("o-1"),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            source_snapshot_id: None,
            previous_revision_id: None,
            content_hash: "abc123def456".to_string(),
            customer_revision_id: Some(PartyRevisionId::new("party-rev-1")),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            snapshot: HeaderSnapshotData {
                customer_name: "东方企业".to_string(),
                contract_no: Some("HT-2026-0088".to_string()),
                settlement_party_name: Some("集团结算中心".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结 30 天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "6".to_string(),
            },
            project_name: Some("端午福利项目".to_string()),
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            effective_at: Instant::from_unix_secs(1_800_000_000),
            recorded_at: Instant::from_unix_secs(1_800_000_100),
        }
    }

    fn revision_line(name: &str, line_no: u32) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new(format!("rl-{line_no}")),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
                line_no,
                line_type: LineType::GoodsService,
                gross_amount: amt("29.97"),
                net_amount: amt("26.07"),
                tax_amount: amt("3.90"),
                sales_tax_rate: Rate::from_str("0.130000").unwrap(),
                item_name_snapshot: name.to_string(),
                spec_snapshot: Some("10kg".to_string()),
                unit_snapshot: Some("盒".to_string()),
            },
        )
        .unwrap()
    }

    #[test]
    fn revision_view_keeps_header_snapshot_and_line_summary() {
        let revision = SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), header_data()).unwrap();
        let view = revision_view(revision, vec![revision_line("年货礼盒", 1)], None);

        assert_eq!(view.revision_no, 1);
        assert_eq!(view.customer_name, "东方企业");
        assert_eq!(view.contract_no.as_deref(), Some("HT-2026-0088"));
        assert_eq!(view.settlement_party_name.as_deref(), Some("集团结算中心"));
        assert_eq!(view.payment_term_name, "月结 30 天");
        assert_eq!(view.line_summary, "年货礼盒 共 1 项");
        assert_eq!(view.lines[0].item_name, "年货礼盒");
        assert_eq!(view.previous_revision_no, None);
    }

    #[test]
    fn revision_line_summary_joins_names_and_count() {
        let lines = vec![revision_line("年货礼盒", 1), revision_line("企业福利卡", 2)];
        assert_eq!(revision_line_summary(&lines), "年货礼盒、企业福利卡 共 2 项");
        assert_eq!(revision_line_summary(&[]), "");
    }
}
