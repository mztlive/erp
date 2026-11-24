//! DTO ↔ 实体/视图映射：构建稳定明细、工作副本、提交快照与视图转换。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{
    SalesOrderId, SalesOrderLineId, SalesOrderSubmissionId, SalesOrderSubmissionLineId,
    SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId,
};
use id_generator::next_id;

use crate::dto::sales_order::{
    RevisionLineView, RevisionView, SalesOrderDraftLineRequest, SalesOrderDraftRequest,
    SalesOrderWorkingCopyLineView, SubmissionView,
};
use crate::entity::sales_order::{
    SalesContentHash, SalesOrder, SalesOrderLine, SalesOrderLineData, SalesOrderRevision,
    SalesOrderRevisionLine, SalesOrderSubmission, SalesOrderSubmissionData, SalesOrderSubmissionLine,
    SalesOrderSubmissionLineData, SalesOrderWorkingCopy, SalesOrderWorkingCopyData,
    SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData, SalesPricingMode, WorkingPurpose,
};
use crate::{Error, Result};

/// 构建稳定明细行（订单创建时按草稿行号建立）。
/// 返回稳定明细行清单（行号升序）。
/// 行号重复时返回错误。
///
/// 输入约定：
/// - `order_id`：所属销售单
/// - `lines`：草稿行请求
pub fn build_stable_lines(
    order_id: &SalesOrderId,
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderLine>> {
    let mut stable = Vec::with_capacity(lines.len());
    let mut seen_line_nos = HashSet::with_capacity(lines.len());
    for line in lines {
        if !seen_line_nos.insert(line.line_no) {
            return Err(Error::ValidationError(format!("行号 {} 重复", line.line_no)));
        }
        stable.push(SalesOrderLine::new(
            SalesOrderLineId::new(next_id()),
            order_id.clone(),
            SalesOrderLineData { line_no: line.line_no },
        )?);
    }
    Ok(stable)
}

/// 构建工作副本（含行实体）。
///
/// 行金额由实体 `new` 逐行舍入计算，表头金额由服务端汇总已舍入的行金额
/// （§4.2 铁律 2）。
/// 返回 `(工作副本实体, 工作副本行清单)`。
/// 表头快照、行字段组或金额三元组校验失败时返回错误。
///
/// 输入约定：
/// - `order`：所属销售单实体（已建）
/// - `stable_lines`：稳定明细行（已建，行号与草稿行一一对应）
/// - `draft`：草稿表头与明细请求
/// - `draft_version`：初始草稿版本
pub fn build_working_copy(
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
    let lines = materialize_working_copy_lines(&working_copy_id, line_datas.iter().cloned())?;
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
            voucher_expiry_at: draft.voucher_expiry_at.map(|secs| Instant::from_unix_secs(secs as i64)),
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

// 将草稿行请求映射为工作副本行创建数据（稳定行号 → 稳定明细身份）。
// 返回工作副本行创建数据清单。
// 行号无对应稳定明细时返回错误。
//
// 输入约定：
// - `stable_lines`：稳定明细行（行号与草稿行一一对应）
// - `lines`：草稿行请求
fn build_working_copy_line_datas(
    stable_lines: &[SalesOrderLine],
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderWorkingCopyLineData>> {
    let mut stable_by_no = HashMap::with_capacity(stable_lines.len());
    for stable in stable_lines {
        stable_by_no.entry(stable.line_no).or_insert(stable);
    }

    let mut datas = Vec::with_capacity(lines.len());
    for line in lines {
        let stable_id = stable_by_no
            .get(&line.line_no)
            .map(|stable| stable.base.id.clone())
            .ok_or_else(|| Error::ValidationError(format!("行号 {} 无对应稳定明细", line.line_no)))?;
        let mut goods = line.goods.clone();
        if let Some(goods) = &mut goods {
            crate::entity::sales_order::types::ensure_positive_sales_price(goods.unit_price_gross)?;
            goods.pricing_mode = SalesPricingMode::Manual;
        }
        datas.push(SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new(stable_id),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods,
            voucher: line.voucher.clone(),
        });
    }
    Ok(datas)
}

// 由行创建数据物化工作副本行实体。
// 返回工作副本行实体清单（金额由实体逐行舍入计算）。
// 行字段组与行类型不一致、金额非法时返回错误。
//
// 输入约定：
// - `working_copy_id`：所属工作副本 ID
// - `line_datas`：行创建数据
fn materialize_working_copy_lines(
    working_copy_id: &SalesOrderWorkingCopyId,
    line_datas: impl IntoIterator<Item = SalesOrderWorkingCopyLineData>,
) -> Result<Vec<SalesOrderWorkingCopyLine>> {
    let line_datas = line_datas.into_iter();
    let mut built = Vec::with_capacity(line_datas.size_hint().0);
    for data in line_datas {
        built.push(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new(next_id()),
            working_copy_id.clone(),
            data,
        )?);
    }
    Ok(built)
}

/// 构建工作副本行实体（草稿保存路径复用：稳定行 + 草稿请求 → 行实体）。
/// 返回工作副本行清单（金额由实体逐行舍入计算）。
/// 行字段组与行类型不一致、金额非法时返回错误。
///
/// 输入约定：
/// - `_order_id`：所属销售单（预留，稳定行 ID 由建单实体产生）
/// - `working_copy_id`：所属工作副本 ID
/// - `stable_lines`：稳定明细行（行号与草稿行一一对应）
/// - `lines`：草稿行请求
pub fn build_working_copy_lines(
    _order_id: &SalesOrderId,
    working_copy_id: &SalesOrderWorkingCopyId,
    stable_lines: &[SalesOrderLine],
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderWorkingCopyLine>> {
    let line_datas = build_working_copy_line_datas(stable_lines, lines)?;
    materialize_working_copy_lines(working_copy_id, line_datas)
}

/// 构建表头快照入参。
/// 返回表头快照入参。
/// 失败条件：字段复制不失败，始终返回 `Ok`。
///
/// 输入约定：
/// - `draft`：草稿表头请求
pub fn header_snapshot(
    draft: &SalesOrderDraftRequest,
) -> Result<crate::entity::sales_order::HeaderSnapshotData> {
    Ok(crate::entity::sales_order::HeaderSnapshotData {
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
/// 返回提交快照实体。
/// 提交字段校验失败时返回错误。
///
/// 输入约定：
/// - `working_copy`：已迁移到 `Submitted` 的工作副本
/// - `lines`：工作副本行
/// - `submission_no`：提交序号
/// - `actor`：提交人
pub fn build_submission(
    working_copy: &SalesOrderWorkingCopy,
    lines: &[SalesOrderWorkingCopyLine],
    submission_no: u32,
    actor: &AuditActor,
) -> Result<SalesOrderSubmission> {
    let data = SalesOrderSubmissionData::from_working_copy(
        working_copy,
        lines,
        submission_no,
        Instant::now(),
        actor.id(),
    )?;
    SalesOrderSubmission::new(SalesOrderSubmissionId::new(next_id()), data).map_err(Error::Logic)
}

/// 从工作副本行构建提交快照明细。
///
/// 行字段组复制由实体工厂完成；本方法只注入提交行 ID。
/// 返回提交快照明细清单。
/// 行字段组缺失或非法时返回错误。
///
/// 输入约定：
/// - `submission`：提交快照
/// - `lines`：工作副本行
pub fn build_submission_lines(
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
/// 返回视图。
///
/// 输入约定：
/// - `submission`：提交快照实体
/// - `lines`：该提交下的明细行
pub fn submission_view(
    submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
) -> SubmissionView {
    SubmissionView {
        id: submission.base.id,
        submission_no: submission.submission_no,
        status: submission.stable.status,
        business_type: submission.business_type,
        customer_name: submission.customer_snapshot.customer_name,
        contract_no: submission.contract_snapshot.map(|s| s.contract_no),
        contract_revision_id: submission.contract_revision_id.as_ref().map(ToString::to_string),
        settlement_party_name: submission.settlement_party_snapshot.map(|s| s.settlement_party_name),
        payment_term_code: submission.payment_term_snapshot.payment_term_code,
        payment_term_name: submission.payment_term_snapshot.payment_term_name,
        invoice_type: submission.invoice_requirement_snapshot.invoice_type,
        tax_point: submission.invoice_requirement_snapshot.tax_point,
        project_name: submission.project_name,
        business_remark: submission.business_remark,
        voucher_category_sku_id: submission.voucher_category_sku_id.as_ref().map(ToString::to_string),
        voucher_expiry_at: submission.voucher_expiry_at.map(|instant| instant.unix_secs() as u64),
        receivable_due_date: submission.receivable_due_date,
        gross_amount: submission.gross_amount,
        net_amount: submission.net_amount,
        tax_amount: submission.tax_amount,
        submitted_by: submission.submitted_by,
        submitted_at: submission.submitted_at.unix_secs() as u64,
        created_at: submission.base.created_at,
        lines: lines.into_iter().map(SalesOrderWorkingCopyLineView::from_submission_line).collect(),
    }
}

/// 构造销售版本历史视图（含当时表头快照与公共行摘要）。
/// 返回详情「版本」分区使用的视图。
/// 业务边界：快照字段只读正式版本内联值，不得用当前客户/合同主数据回填。
///
/// 输入约定：
/// - `revision`：正式销售版本实体
/// - `lines`：该版本下的公共行；调用方须已按行号升序
/// - `previous_revision_no`：前一生效版本号；无法在本单版本列表解析时为空
/// - `commercial_lines`：同版公共行与子类型快照组装的完整成交明细
pub fn revision_view(
    revision: SalesOrderRevision,
    lines: Vec<SalesOrderRevisionLine>,
    previous_revision_no: Option<u32>,
    commercial_lines: Vec<SalesOrderWorkingCopyLineView>,
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
        voucher_category_sku_id: revision.voucher_category_sku_id.as_ref().map(ToString::to_string),
        voucher_expiry_at: revision.voucher_expiry_at.map(|instant| instant.unix_secs() as u64),
        previous_revision_id: revision.previous_revision_id.as_ref().map(ToString::to_string),
        previous_revision_no,
        gross_amount: revision.gross_amount,
        net_amount: revision.net_amount,
        tax_amount: revision.tax_amount,
        effective_at: revision.effective_at.unix_secs() as u64,
        created_at: revision.base.created_at,
        line_summary: revision_line_summary(&lines),
        lines: lines.into_iter().map(revision_line_view).collect(),
        commercial_lines,
    }
}

// 把公共行版本收成详情页一行摘要。
// 返回行号、名称、规格、单位与含税金额。
// 业务边界：不展开实物/卡券子类型字段，版本列表只展示公共行快照。
//
// 输入约定：
// - `line`：公共行版本实体
fn revision_line_view(line: SalesOrderRevisionLine) -> RevisionLineView {
    RevisionLineView {
        line_no: line.line_no,
        item_name: line.item_name_snapshot,
        spec: line.spec_snapshot,
        unit: line.unit_snapshot,
        gross_amount: line.gross_amount,
    }
}

// 把版本公共行收成一条可读摘要。
// 结果：无行时返回空串；有行时返回「名称、名称 共 N 项」。
// 业务边界：名称顺序与传入行顺序一致，调用方须先按行号排序。
//
// 输入约定：
// - `lines`：已按行号排序的公共行
fn revision_line_summary(lines: &[SalesOrderRevisionLine]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let names = lines.iter().map(|line| line.item_name_snapshot.as_str()).collect::<Vec<_>>();
    format!("{} 共 {} 项", names.join("、"), lines.len())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::time::Instant as Stopwatch;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ContractRevisionId, PartyRevisionId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, SalesOrderWorkingCopyId,
    };
    use erp_core::money::{Amount, Rate};

    use super::{
        build_stable_lines, build_working_copy_line_datas, build_working_copy_lines, revision_line_summary,
        revision_view,
    };
    use crate::Error;
    use crate::dto::sales_order::SalesOrderDraftLineRequest;
    use crate::entity::sales_order::{
        HeaderSnapshotData, LineType, RevisionSource, SalesOrderRevision, SalesOrderRevisionData,
        SalesOrderRevisionLine, SalesOrderRevisionLineData, SalesPricingMode,
    };

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn header_data() -> SalesOrderRevisionData {
        SalesOrderRevisionData {
            settlement_party_id: "party-1".to_string().into(),
            sales_order_id: SalesOrderId::new("o-1"),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
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
        let view = revision_view(revision, vec![revision_line("年货礼盒", 1)], None, Vec::new());

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

    // 组装用于草稿身份映射的行请求，不执行金额物化。
    fn draft_line(line_no: u32) -> SalesOrderDraftLineRequest {
        SalesOrderDraftLineRequest {
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: Rate::from_str("0.13").unwrap(),
            item_name_snapshot: format!("商品-{line_no}"),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: None,
            voucher: None,
        }
    }

    // 回归检查：重复稳定行取第一条；输出保持请求顺序及重复请求，不按索引顺序输出。
    #[test]
    fn draft_mapping_uses_first_matching_identity_and_preserves_request_order() {
        let requests = [draft_line(2), draft_line(1), draft_line(2)];
        let mut stable = build_stable_lines(&SalesOrderId::new("order-1"), &requests[..2]).unwrap();
        let mut duplicate = stable[0].clone();
        duplicate.base.id = "later-duplicate".into();
        stable.push(duplicate);

        let result = build_working_copy_line_datas(&stable, &requests).unwrap();
        assert_eq!(
            result.iter().map(|line| line.sales_order_line_id.as_ref()).collect::<Vec<_>>(),
            [stable[0].base.id.as_str(), stable[1].base.id.as_str(), stable[0].base.id.as_str()]
        );
        assert_eq!(result.iter().map(|line| line.line_no).collect::<Vec<_>>(), [2, 1, 2]);
        assert!(build_working_copy_line_datas(&stable, &[]).unwrap().is_empty());
    }

    // 组装带成交模式和采购供给价的实物草稿行，覆盖映射后的金额校验。
    fn priced_draft_line(mode: &str, price: &str) -> SalesOrderDraftLineRequest {
        let mut line = draft_line(1);
        line.goods = Some(
            serde_json::from_value(serde_json::json!({
                "sku_id":"sku-1", "sku_revision_id":"sku-rev-1", "welfare_scenario":null,
                "fulfillment_due_at":1800000000, "quantity":"3",
                "base_unit_code":"件", "unit_price_gross":price, "pricing_mode":mode,
                "procurement_supply_price_gross":"13.72"
            }))
            .unwrap(),
        );
        line
    }

    // 回归检查：成交价及供给成本保留，旧自动模式转为手工模式。
    #[test]
    fn working_copy_mapping_preserves_entered_prices() {
        for (mode, price, expected_mode, gross) in [
            ("MANUAL", "37.12", SalesPricingMode::Manual, "111.36"),
            ("AUTO", "37.12", SalesPricingMode::Manual, "111.36"),
        ] {
            let line = priced_draft_line(mode, price);
            let order_id = SalesOrderId::new("order-1");
            let stable = build_stable_lines(&order_id, std::slice::from_ref(&line)).unwrap();
            let rows = build_working_copy_lines(
                &order_id,
                &SalesOrderWorkingCopyId::new("copy-1"),
                &stable,
                &[line],
            )
            .unwrap();
            assert_eq!(rows[0].pricing_mode, expected_mode);
            assert_eq!(rows[0].unit_price_gross, Some(price.parse().unwrap()));
            assert_eq!(rows[0].gross_amount, amt(gross));
            assert_eq!(rows[0].procurement_supply_price_gross, Some("13.72".parse().unwrap()));
        }
    }

    // 草稿保存对所有成交方式均要求正价。
    #[test]
    fn working_copy_mapping_rejects_nonpositive_sales_prices() {
        for (mode, price, message) in
            [("MANUAL", "0", "含税成交单价必须大于 0"), ("AUTO", "0", "含税成交单价必须大于 0")]
        {
            let line = priced_draft_line(mode, price);
            let order_id = SalesOrderId::new("order-1");
            let stable = build_stable_lines(&order_id, std::slice::from_ref(&line)).unwrap();
            let error = build_working_copy_lines(
                &order_id,
                &SalesOrderWorkingCopyId::new("copy-1"),
                &stable,
                &[line],
            )
            .unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn draft_mapping_reports_first_missing_requested_number() {
        let error = build_working_copy_line_datas(&[], &[draft_line(9), draft_line(3)]).unwrap_err();
        assert!(matches!(error, Error::ValidationError(ref message)
            if message == "行号 9 无对应稳定明细"));
    }

    // 回归检查：大批草稿行按请求顺序匹配稳定身份。
    #[test]
    fn large_draft_mapping_sample() {
        let mut lines = (1..=2000).map(draft_line).collect::<Vec<_>>();
        let stable = build_stable_lines(&SalesOrderId::new("order-1"), &lines).unwrap();
        lines.reverse();

        let started = Stopwatch::now();
        for _ in 0..20 {
            let mapped = build_working_copy_line_datas(&stable, &lines).unwrap();
            assert_eq!(mapped.first().unwrap().sales_order_line_id.as_ref(), stable[1999].base.id);
            assert_eq!(mapped.last().unwrap().sales_order_line_id.as_ref(), stable[0].base.id);
            assert_eq!(mapped.first().unwrap().item_name_snapshot, "商品-2000");
        }
        eprintln!("sales_2000_draft_lines_20_runs={:?}", started.elapsed());
    }
}
