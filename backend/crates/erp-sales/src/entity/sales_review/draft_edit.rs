//! 销售变更原单修改规则；已提交副本保留，重建可编辑副本。

use std::collections::HashSet;

use entity_core::BaseModel;
use erp_core::common::stable::StableBase;
use erp_core::ids::{SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId};

use super::{SalesChangeOrder, SalesChangeOrderUpdate, SalesChangeSubmission};
use crate::entity::sales_order::{
    SalesContentHash, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData,
    SalesOrderWorkingCopyUpdate, WorkingCopyStatus, WorkingPurpose,
};
use crate::{Error, Result};

impl SalesChangeOrder {
    /// 在原单版本与草稿状态成立时修改原因。
    ///
    /// # 参数
    /// * `expected_version` - 页面读取的变更单版本
    /// * `reason` - 变更原因
    /// * `actor_id` - 当前编辑人
    ///
    /// # 返回
    /// 返回更新后的草稿事实。
    ///
    /// # 错误
    /// 版本变化、未撤回审批或原因非法时拒绝。
    pub fn edit_draft(&mut self, expected_version: u64, reason: String, actor_id: &str) -> Result<()> {
        if !self.matches_version(expected_version) {
            return Err(Error::ConflictError("销售变更单已变化，请刷新后重试".into()));
        }
        if !self.is_draft() {
            return Err(Error::ConflictError("请先撤回销售变更审批，再修改原单".into()));
        }
        self.update(SalesChangeOrderUpdate { reason: Some(reason), change_type: None }, actor_id)?;
        Ok(())
    }
}

impl SalesOrderWorkingCopy {
    /// 为同一销售变更准备可编辑副本，已提交副本仅复制为新草稿。
    ///
    /// # 参数
    /// * `change` - 已核验为草稿的原销售变更单
    /// * `expected_version` - 工作副本版本
    /// * `new_id` - 已提交副本重新编辑时的新身份
    /// * `actor_id` - 当前编辑人
    ///
    /// # 返回
    /// 返回保留原单与基准身份的可编辑副本。
    ///
    /// # 错误
    /// 引用不一致、状态非法、版本冲突时拒绝。
    pub fn editable_change_copy(
        &self,
        change: &SalesChangeOrder,
        expected_version: u64,
        new_id: SalesOrderWorkingCopyId,
        actor_id: &str,
    ) -> Result<Self> {
        self.ensure_change_editable(change)?;
        if !self.matches_version(expected_version) {
            return Err(Error::ConflictError("变更内容已变化，请刷新后重试".into()));
        }
        let mut copy = self.clone();
        match self.stable.status {
            WorkingCopyStatus::Editing => {},
            WorkingCopyStatus::Submitted => {
                copy.base = BaseModel::new(new_id.to_string());
                copy.stable = StableBase::new(WorkingCopyStatus::Editing, actor_id);
            },
            _ => return Err(Error::ConflictError("当前变更内容不可编辑".into())),
        }
        Ok(copy)
    }

    /// 证明工作副本属于原草稿变更，并且其内容允许重新编辑。
    ///
    /// # 参数
    /// * `change` - 已读取的原销售变更单
    ///
    /// # 返回
    /// 身份、基准及状态均匹配时成功。
    ///
    /// # 错误
    /// 引用不一致、变更不是草稿或副本状态不可编辑时拒绝。
    pub fn ensure_change_editable(&self, change: &SalesChangeOrder) -> Result<()> {
        if !change.is_draft()
            || self.working_purpose != WorkingPurpose::SalesChange
            || self.sales_change_order_id.as_ref().map(AsRef::as_ref) != Some(change.base.id.as_str())
            || self.sales_order_id != change.sales_order_id
            || self.base_revision_id.as_ref() != Some(&change.base_revision_id)
            || !matches!(self.stable.status, WorkingCopyStatus::Editing | WorkingCopyStatus::Submitted)
        {
            return Err(Error::ConflictError("销售变更工作副本与原单或状态不一致".into()));
        }
        Ok(())
    }

    /// 完整保存目标明细并重算金额，禁止丢失或替换稳定明细身份。
    ///
    /// # 参数
    /// * `change_id` - 原销售变更单身份
    /// * `original` - 已读取的全部原明细
    /// * `target` - 完整目标明细
    /// * `remark` - 业务备注
    /// * `actor_id` - 当前编辑人
    ///
    /// # 返回
    /// 返回经过实体构造校验的目标明细。
    ///
    /// # 错误
    /// 明细重复、缺失、金额非法或草稿版本溢出时拒绝。
    pub fn edit_change_lines(
        &mut self,
        change_id: &str,
        original: &[SalesOrderWorkingCopyLine],
        target: Vec<(SalesOrderWorkingCopyLineId, SalesOrderWorkingCopyLineData)>,
        remark: Option<String>,
        actor_id: &str,
    ) -> Result<Vec<SalesOrderWorkingCopyLine>> {
        validate_target_lines(self.business_type, original, &target)?;
        let next = self
            .draft_version
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("草稿版本已达上限".into()))?;
        let lines = target
            .into_iter()
            .map(|(line_id, data)| {
                SalesOrderWorkingCopyLine::new(line_id, SalesOrderWorkingCopyId::new(&self.base.id), data)
                    .map_err(Error::from)
            })
            .collect::<Result<Vec<_>>>()?;
        let (gross, net, tax) = SalesOrderWorkingCopyLine::amount_totals(&lines);
        let mut edited = self.clone();
        edited.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(SalesContentHash::change(change_id, next)?.into_wire()),
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
                business_remark: Some(remark.unwrap_or_default()),
                ..Default::default()
            },
            actor_id,
        )?;
        edited.draft_version = next;
        edited.editor_user_id = actor_id.to_string();
        *self = edited;
        Ok(lines)
    }
}

/// 校验完整目标保留稳定明细身份，并满足业务性质和行号约束。
///
/// # 参数
/// * `business_type` - 原销售单不可变业务性质
/// * `original` - 原草稿完整明细
/// * `target` - 本次完整目标明细
///
/// # 返回
/// 明细身份、数量及跨行约束均成立时成功。
///
/// # 错误
/// 原明细缺失、目标重复、行性质或行号非法时拒绝。
fn validate_target_lines(
    business_type: crate::entity::sales_order::BusinessType,
    original: &[SalesOrderWorkingCopyLine],
    target: &[(SalesOrderWorkingCopyLineId, SalesOrderWorkingCopyLineData)],
) -> Result<()> {
    let original_ids = original.iter().map(|line| line.sales_order_line_id.as_ref()).collect::<HashSet<_>>();
    let target_ids = target.iter().map(|(_, line)| line.sales_order_line_id.as_ref()).collect::<HashSet<_>>();
    if target.is_empty()
        || target.len() > 100
        || target_ids.len() != target.len()
        || original_ids != target_ids
    {
        return Err(Error::ValidationError("变更目标必须保留全部原明细，且不得重复".into()));
    }
    let summaries = target
        .iter()
        .map(|(_, line)| crate::entity::sales_order::types::LineSummary {
            line_no: line.line_no,
            line_id: line.sales_order_line_id.clone(),
            line_type: line.line_type,
        })
        .collect::<Vec<_>>();
    Ok(crate::entity::sales_order::types::validate_line_list(business_type, &summaries)?)
}

impl SalesChangeSubmission {
    /// 证明当前提交来自原变更被冻结的同一工作副本版本。
    ///
    /// # 参数
    /// * `change` - 原销售变更单
    /// * `copy` - 当前提交引用的工作副本
    ///
    /// # 返回
    /// 返回完整目标的不可变内容身份。
    ///
    /// # 错误
    /// 原单、基准、工作副本或冻结版本错配，及副本不是已提交时拒绝。
    pub fn saved_target_content_hash<'a>(
        &self,
        change: &SalesChangeOrder,
        copy: &'a SalesOrderWorkingCopy,
    ) -> Result<&'a str> {
        if self.sales_change_order_id.as_ref() != change.base.id.as_str()
            || self.sales_order_id != change.sales_order_id
            || self.base_revision_id != change.base_revision_id
            || self.working_copy_id.as_ref() != copy.base.id.as_str()
            || copy.working_purpose != WorkingPurpose::SalesChange
            || !copy.is_submitted()
            || copy.draft_version != self.working_copy_version
            || copy.sales_change_order_id.as_ref().map(AsRef::as_ref) != Some(change.base.id.as_str())
            || copy.sales_order_id != change.sales_order_id
            || copy.base_revision_id.as_ref() != Some(&change.base_revision_id)
        {
            return Err(Error::ConflictError("销售变更提交来源或冻结版本不一致".into()));
        }
        Ok(&copy.content_hash)
    }
}

impl SalesOrderWorkingCopyLine {
    /// 把完整工作副本行转成原单编辑数据，保留冻结商品身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可再次由实体构造函数校验的完整行。
    ///
    /// # 错误
    /// 行字段不符合其业务性质时返回校验错误。
    pub fn draft_data(&self) -> Result<SalesOrderWorkingCopyLineData> {
        Ok(SalesOrderWorkingCopyLineData {
            sales_order_line_id: self.sales_order_line_id.clone(),
            line_no: self.line_no,
            line_type: self.line_type,
            sales_tax_rate: self.sales_tax_rate,
            item_name_snapshot: self.item_name_snapshot.clone(),
            spec_snapshot: self.spec_snapshot.clone(),
            unit_snapshot: self.unit_snapshot.clone(),
            goods: self.goods_fields()?,
            voucher: self.voucher_fields()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        CustomerAccountId, PartyId, SalesChangeOrderId, SalesChangeSubmissionId, SalesOrderId,
        SalesOrderLineId, SalesOrderRevisionId, SkuId, SkuRevisionId,
    };
    use erp_core::money::Amount;

    use super::*;
    use crate::entity::sales_order::{
        BusinessType, CardForm, GoodsLineFields, HeaderSnapshotData, LineType, SalesOrderWorkingCopyData,
        VoucherLineDraft,
    };

    fn amt(value: &str) -> Amount {
        value.parse().unwrap()
    }

    fn line_data(line_no: u32) -> SalesOrderWorkingCopyLineData {
        SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: "0.13".parse().unwrap(),
            item_name_snapshot: "年货礼盒".into(),
            spec_snapshot: Some("10kg".into()),
            unit_snapshot: Some("箱".into()),
            goods: Some(GoodsLineFields {
                pricing_mode: Default::default(),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("sku-rev-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: "3".parse().unwrap(),
                base_unit_code: "箱".into(),
                unit_price_gross: "9.99".parse().unwrap(),
            }),
            voucher: None,
        }
    }
    use crate::entity::sales_review::{SalesChangeOrderData, SalesChangeType};

    fn fixture() -> (SalesChangeOrder, SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>) {
        let change = SalesChangeOrder::new(
            SalesChangeOrderId::new("change-1"),
            SalesChangeOrderData {
                sales_order_id: SalesOrderId::new("order-1"),
                base_revision_id: SalesOrderRevisionId::new("revision-1"),
                change_type: SalesChangeType::Quantity,
                reason: "修改数量".into(),
            },
            "sales-1",
        )
        .unwrap();
        let data = line_data(1);
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("copy-line-1"),
            SalesOrderWorkingCopyId::new("copy-1"),
            data.clone(),
        )
        .unwrap();
        let copy = SalesOrderWorkingCopy::new(
            SalesOrderWorkingCopyId::new("copy-1"),
            SalesOrderWorkingCopyData {
                sales_order_id: SalesOrderId::new("order-1"),
                working_purpose: WorkingPurpose::SalesChange,
                sales_change_order_id: Some(SalesChangeOrderId::new("change-1")),
                base_revision_id: Some(SalesOrderRevisionId::new("revision-1")),
                draft_version: 1,
                content_hash: "hash-1".into(),
                editor_user_id: "sales-1".into(),
                business_type: BusinessType::GoodsService,
                customer_id: CustomerAccountId::new("customer-1"),
                contract_id: None,
                contract_revision_id: None,
                settlement_party_id: PartyId::new("party-1"),
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".into(),
                    contract_no: None,
                    settlement_party_name: Some("结算主体".into()),
                    payment_term_code: "NET30".into(),
                    payment_term_name: "月结".into(),
                    invoice_type: "普通发票".into(),
                    tax_point: "13".into(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                receivable_due_date: None,
                gross_amount: amt("29.97"),
                net_amount: amt("26.07"),
                tax_amount: amt("3.90"),
                lines: vec![data],
            },
            "sales-1",
        )
        .unwrap();
        (change, copy, vec![line])
    }

    #[test]
    fn withdrawn_change_edits_new_copy_and_preserves_submitted_content() {
        let (mut change, mut original, lines) = fixture();
        original.submit().unwrap();
        let frozen = original.clone();
        change.edit_draft(change.base.version, "补充数量".into(), "sales-1").unwrap();
        let mut copy = original
            .editable_change_copy(
                &change,
                original.base.version,
                SalesOrderWorkingCopyId::new("copy-2"),
                "sales-1",
            )
            .unwrap();
        let mut target = lines[0].draft_data().unwrap();
        target.goods.as_mut().unwrap().quantity = "4".parse().unwrap();
        let saved = copy
            .edit_change_lines(
                "change-1",
                &lines,
                vec![(SalesOrderWorkingCopyLineId::new("line-2"), target)],
                Some("修改".into()),
                "sales-1",
            )
            .unwrap();
        assert_eq!(original, frozen);
        assert_eq!(copy.stable.status, WorkingCopyStatus::Editing);
        assert_eq!(copy.draft_version, 2);
        assert_eq!(copy.gross_amount, amt("39.96"));
        assert_eq!(saved[0].sales_order_line_id, lines[0].sales_order_line_id);
        assert_eq!(change.base.id, "change-1");
    }

    #[test]
    fn draft_edit_rejects_in_approval_stale_version_and_changed_line_identity() {
        let (mut change, mut copy, lines) = fixture();
        let before = change.clone();
        assert!(change.edit_draft(change.base.version + 1, "新原因".into(), "sales-1").is_err());
        assert_eq!(change, before);
        change
            .start_approval(SalesChangeSubmissionId::new("submission-1"), "target-hash", "sales-1")
            .unwrap();
        assert!(change.edit_draft(change.base.version, "新原因".into(), "sales-1").is_err());
        let mut target = lines[0].draft_data().unwrap();
        target.sales_order_line_id = SalesOrderLineId::new("foreign-line");
        let before_copy = copy.clone();
        assert!(
            copy.edit_change_lines(
                "change-1",
                &lines,
                vec![(SalesOrderWorkingCopyLineId::new("new-line"), target)],
                None,
                "sales-1"
            )
            .is_err()
        );
        assert_eq!(copy, before_copy);
    }

    #[test]
    fn submitted_target_requires_exact_frozen_copy_and_version() {
        let (change, mut copy, lines) = fixture();
        let data = super::super::SalesChangeSubmissionData::from_sales_working_copy(
            &change,
            &copy,
            &lines,
            1,
            Instant::from_unix_secs(1_800_000_000),
            "sales-1",
        )
        .unwrap();
        let mut submission =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("submission-1"), data).unwrap();
        assert!(
            submission.saved_target_content_hash(&change, &copy).is_err(),
            "可编辑副本不能证明已提交目标"
        );
        copy.submit().unwrap();
        assert_eq!(submission.saved_target_content_hash(&change, &copy).unwrap(), copy.content_hash);
        submission.working_copy_version += 1;
        assert!(submission.saved_target_content_hash(&change, &copy).is_err());
        submission.working_copy_version -= 1;
        submission.working_copy_id = SalesOrderWorkingCopyId::new("other-copy");
        assert!(submission.saved_target_content_hash(&change, &copy).is_err());
        submission.working_copy_id = SalesOrderWorkingCopyId::new("copy-1");
        submission.sales_order_id = SalesOrderId::new("other-order");
        assert!(submission.saved_target_content_hash(&change, &copy).is_err());
    }

    #[test]
    fn editable_copy_rejects_wrong_source_state_and_stale_working_copy_version() {
        let (change, mut copy, _) = fixture();
        assert!(
            copy.editable_change_copy(
                &change,
                copy.base.version + 1,
                SalesOrderWorkingCopyId::new("new"),
                "sales-1"
            )
            .is_err()
        );
        copy.sales_change_order_id = Some(SalesChangeOrderId::new("other-change"));
        assert!(copy.ensure_change_editable(&change).is_err());
        copy.sales_change_order_id = Some(SalesChangeOrderId::new("change-1"));
        copy.base_revision_id = Some(SalesOrderRevisionId::new("other-revision"));
        assert!(copy.ensure_change_editable(&change).is_err());
        copy.base_revision_id = Some(SalesOrderRevisionId::new("revision-1"));
        copy.stable.status = WorkingCopyStatus::Conflict;
        assert!(copy.ensure_change_editable(&change).is_err());
    }

    #[test]
    fn draft_edit_rejects_duplicate_lines_and_version_overflow() {
        let (_, mut copy, lines) = fixture();
        let data = lines[0].draft_data().unwrap();
        assert!(
            copy.edit_change_lines(
                "change-1",
                &lines,
                vec![
                    (SalesOrderWorkingCopyLineId::new("line-2"), data.clone()),
                    (SalesOrderWorkingCopyLineId::new("line-3"), data.clone())
                ],
                None,
                "sales-1"
            )
            .is_err()
        );
        copy.draft_version = u32::MAX;
        assert!(
            copy.edit_change_lines(
                "change-1",
                &lines,
                vec![(SalesOrderWorkingCopyLineId::new("line-2"), data)],
                None,
                "sales-1"
            )
            .is_err()
        );
    }

    #[test]
    fn draft_edit_preserves_stable_line_and_zero_amounts_for_zero_quantity() {
        let (_, mut copy, lines) = fixture();
        let mut target = lines[0].draft_data().unwrap();
        target.goods.as_mut().unwrap().quantity = "0".parse().unwrap();
        let saved = copy
            .edit_change_lines(
                "change-1",
                &lines,
                vec![(SalesOrderWorkingCopyLineId::new("line-2"), target)],
                None,
                "sales-1",
            )
            .unwrap();
        assert_eq!(saved[0].sales_order_line_id, lines[0].sales_order_line_id);
        assert_eq!(saved[0].quantity, Some("0".parse().unwrap()));
        assert_eq!(saved[0].gross_amount, Amount::zero());
        assert_eq!(saved[0].net_amount, Amount::zero());
        assert_eq!(saved[0].tax_amount, Amount::zero());
        assert_eq!(copy.gross_amount, Amount::zero());
        assert_eq!(copy.net_amount, Amount::zero());
        assert_eq!(copy.tax_amount, Amount::zero());
        assert_eq!(copy.draft_version, 2);
    }

    #[test]
    fn draft_edit_rejects_inconsistent_voucher_money_without_mutating_copy() {
        let (_, mut copy, _) = fixture();
        copy.business_type = BusinessType::Voucher;
        let mut data = line_data(1);
        data.line_type = LineType::Voucher;
        data.goods = None;
        data.voucher = Some(VoucherLineDraft {
            face_value: amt("100"),
            card_count: 3,
            unit_price_gross: "90".parse().unwrap(),
            face_value_total: amt("300"),
            transaction_amount: amt("270"),
            gift_amount: amt("30"),
            gift_rate: None,
            card_form: CardForm::Electronic,
        });
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("copy-line-1"),
            SalesOrderWorkingCopyId::new("copy-1"),
            data.clone(),
        )
        .unwrap();
        data.voucher.as_mut().unwrap().transaction_amount = amt("269");
        let before = copy.clone();
        assert!(
            copy.edit_change_lines(
                "change-1",
                &[line],
                vec![(SalesOrderWorkingCopyLineId::new("line-2"), data)],
                None,
                "sales-1"
            )
            .is_err()
        );
        assert_eq!(copy, before);
    }
}
