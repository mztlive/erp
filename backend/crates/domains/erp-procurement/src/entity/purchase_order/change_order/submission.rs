//! `purchase_change_submission` 采购变更提交（数据模型 §6.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{
    PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderRevisionId, SupplierAccountId,
    SupplierCommercialProfileRevisionId,
};
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::PaymentSubjectSnapshot;
use crate::entity::purchase_order::line_amounts::ensure_header_triple;
use crate::entity::purchase_order::purchase_submission::SubmissionStatus;
use crate::entity::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
use crate::entity::purchase_order::types::{FulfillmentResponsibility, PurchaseType};

/// 提交序号最大长度。
const SUBMISSION_NO_MAX_LEN: usize = 64;
/// 操作人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 采购变更提交创建数据（不含系统字段）。
///
/// 字段与 `purchase_order_submission` 相同，并增加
/// `purchase_change_order_id`、`submission_no`、`base_revision_id`（§6.6）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmissionData {
    /// 所属采购变更单。
    pub purchase_change_order_id: PurchaseChangeOrderId,
    /// 提交序号（聚合内唯一）。
    pub submission_no: String,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 供应商（拆单维度）。
    pub supplier_id: SupplierAccountId,
    /// 采购类型（拆单维度）。
    pub purchase_type: PurchaseType,
    /// 履约责任（拆单维度）。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 提交时供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 提交时供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 来源正式销售版本冻结的我方付款主体与来源身份。
    pub payment_subject: PaymentSubjectSnapshot,
    /// 付款条件和先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
}

/// 采购变更提交实体（不可变提交，数据模型 §6.6）。
///
/// 仓储影响确认与财务复核均引用该不可变提交；修改内容必须新建提交并使旧复核失效。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseChangeSubmission {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购变更单。
    pub purchase_change_order_id: PurchaseChangeOrderId,
    /// 提交序号。
    pub submission_no: String,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 提交时供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 提交时供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 来源正式销售版本冻结的我方付款主体与来源身份。
    pub payment_subject: PaymentSubjectSnapshot,
    /// 付款条件和先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
    /// 提交状态（与采购提交同字典：草稿、待审核、已通过、已驳回、因重新提交失效）。
    pub status: SubmissionStatus,
    /// 提交审计时间；与 `submitted_by` 成对出现。
    pub submitted_at: Option<Instant>,
    /// 提交审计人；与 `submitted_at` 成对出现。
    pub submitted_by: Option<String>,
}

impl PurchaseChangeSubmission {
    /// 创建采购变更提交。
    ///
    /// 完成 `submission_no` 校验与规范化，并强制表头金额守恒（`gross = net + tax`，§4.2 铁律 4）。
    ///
    /// 返回新建的提交实体（初始状态 `Draft`）。提交序号为空/超长，或表头金额三元组不守恒时返回错误。
    ///
    /// `id` 为实体主键（`erp_core::ids::PurchaseChangeSubmissionId`）。
    pub fn new(id: PurchaseChangeSubmissionId, data: PurchaseChangeSubmissionData) -> Result<Self> {
        let submission_no = normalize_submission_no(data.submission_no)?;
        ensure_header_triple(data.gross_amount, data.net_amount, data.tax_amount, || {
            format!("变更提交表头金额三元组不守恒（{submission_no}）")
        })?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_change_order_id: data.purchase_change_order_id,
            submission_no,
            base_revision_id: data.base_revision_id,
            supplier_id: data.supplier_id,
            purchase_type: data.purchase_type,
            fulfillment_responsibility: data.fulfillment_responsibility,
            supplier_revision_id: data.supplier_revision_id,
            supplier_snapshot: data.supplier_snapshot,
            payment_subject: data.payment_subject,
            payment_term_snapshot: data.payment_term_snapshot,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            status: SubmissionStatus::Draft,
            submitted_at: None,
            submitted_by: None,
        })
    }

    /// 计算同一变更单的下一个提交序号。
    ///
    /// 仅识别 `CS-{n}` 形态的历史提交，忽略草稿或旧格式编号；新编号固定为六位十进制序号。
    ///
    /// 返回下一个 `CS-000001` 形态的提交序号。最大合法序号已经达到 `u32::MAX` 时返回领域错误。
    ///
    /// `existing` 为同一采购变更单的既有提交。
    pub fn next_submission_no(existing: &[Self]) -> Result<String> {
        let max_no = existing
            .iter()
            .filter_map(|submission| parse_sequence(&submission.submission_no, "CS-"))
            .max()
            .unwrap_or(0);
        let next = max_no.checked_add(1).ok_or_else(|| Error::from("采购变更提交序号溢出"))?;
        Ok(format!("CS-{next:06}"))
    }

    /// 校验变更提交仍处于待处理状态。
    ///
    /// 待审核状态返回 `Ok(())`。提交已经处理、失效或仍是草稿时返回领域错误。
    pub fn ensure_pending(&self) -> Result<()> {
        if self.status != SubmissionStatus::Pending {
            return Err(Error::from("变更提交已处理，请勿重复生效"));
        }
        Ok(())
    }

    /// 记录采购变更最终通过结论。
    ///
    /// 待审核提交成功改为已通过时返回 `Ok(())`。提交不是待审核状态时返回领域错误。
    pub fn approve(&mut self) -> Result<()> {
        self.ensure_pending()?;
        self.status = SubmissionStatus::Approved;
        Ok(())
    }

    /// 提交复核。
    ///
    /// 从草稿进入待审核并写入提交审计；提交后头行冻结。
    ///
    /// 状态不是草稿，或提交人为空、超长时返回错误。
    pub fn submit(&mut self, submitted_at: Instant, submitted_by: impl Into<String>) -> Result<()> {
        if self.status != SubmissionStatus::Draft {
            return Err(Error::from("只有草稿状态的提交可以提交复核"));
        }
        self.status = SubmissionStatus::Pending;
        self.submitted_at = Some(submitted_at);
        self.submitted_by = Some(normalize_required_text(
            submitted_by.into(),
            "提交人不能为空",
            ACTOR_MAX_LEN,
            "提交人标识过长",
        )?);
        Ok(())
    }
}

// 解析带固定前缀的十进制序号。编号匹配前缀且后缀可解析为 `u32` 时返回序号，否则返回 `None`。
fn parse_sequence(value: &str, prefix: &str) -> Option<u32> {
    value.strip_prefix(prefix)?.parse().ok()
}

// 规范化提交序号。
//
// 返回去空白后的提交序号。序号为空或超长时返回错误。
//
// `submission_no` 为原始提交序号。
fn normalize_submission_no(submission_no: String) -> Result<String> {
    normalize_required_text(submission_no, "提交序号不能为空", SUBMISSION_NO_MAX_LEN, "提交序号过长")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
        SupplierAccountId, SupplierCommercialProfileRevisionId,
    };
    use erp_core::money::Amount;

    use super::{PurchaseChangeSubmission, PurchaseChangeSubmissionData};
    use crate::entity::purchase_order::purchase_submission::SubmissionStatus;
    use crate::entity::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
    use crate::entity::purchase_order::types::{FulfillmentResponsibility, PurchaseType};
    use crate::entity::purchase_order::{
        PaymentSubjectSnapshot, PurchaseChangeOrder, PurchaseChangeOrderData,
    };

    fn snapshot() -> SupplierSnapshot {
        SupplierSnapshot::new("北京华联供应商".to_string()).unwrap()
    }

    fn payment_term() -> PaymentTermSnapshot {
        PaymentTermSnapshot::new(
            "NET-30".to_string(),
            false,
            None,
            None,
            crate::entity::test_support::payment_term,
        )
        .unwrap()
    }

    fn change_submission_data() -> PurchaseChangeSubmissionData {
        PurchaseChangeSubmissionData {
            payment_subject: PaymentSubjectSnapshot::new(
                "party-1".to_string().into(),
                "我方公司".into(),
                "so-1".to_string().into(),
                "sales-rev-1".to_string().into(),
            )
            .unwrap(),
            purchase_change_order_id: PurchaseChangeOrderId::new("pco-1"),
            submission_no: "CS-01".to_string(),
            base_revision_id: PurchaseOrderRevisionId::new("por-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
            supplier_snapshot: snapshot(),
            payment_term_snapshot: payment_term(),
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
        }
    }

    // 独立保存保留驳回状态、旧审批版本和历史提交，正式序号仅重提时推进。
    #[test]
    fn save_rejected_target_preserves_history_and_approval_version() {
        use super::super::line::tests::change_line_data;
        use crate::entity::purchase_order::{PurchaseChangeOrderStatus, PurchaseChangeSubmissionLine};

        let mut change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "修改采购".into(),
            },
            "buyer",
        )
        .unwrap();
        let mut old =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("old"), change_submission_data())
                .unwrap();
        old.submit(Instant::from_unix_secs(1_700_000_000), "buyer").unwrap();
        change.start_approval(PurchaseChangeSubmissionId::new("old"), "old-hash", "buyer").unwrap();
        change.reject_approval(1, "approver").unwrap();
        let history = old.clone();
        assert!(old.mark_editing_copy().is_err());
        assert_eq!(old, history);

        let mut draft =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-1"), change_submission_data())
                .unwrap();
        draft.mark_editing_copy().unwrap();
        let lines = vec![
            PurchaseChangeSubmissionLine::new("draft-line".to_string().into(), change_line_data()).unwrap(),
        ];
        change
            .save_draft_target(&draft, &lines, Some("补充成本".into()), "new-hash".into(), "buyer")
            .unwrap();
        assert_eq!(change.stable.status, PurchaseChangeOrderStatus::Rejected);
        assert_eq!(change.approval_subject_version, 1);
        assert_eq!(change.current_submission_id.as_ref().unwrap().as_ref(), "pcs-1");
        assert_eq!(old, history);
        assert_eq!(draft.status, SubmissionStatus::Draft);
        assert!(draft.submitted_at.is_none());
        assert_eq!(PurchaseChangeSubmission::next_submission_no(&[old, draft.clone()]).unwrap(), "CS-000002");

        let before = change.clone();
        assert!(change.save_draft_target(&draft, &[], None, "invalid".into(), "buyer").is_err());
        assert_eq!(change, before);

        change.start_approval(PurchaseChangeSubmissionId::new("new-formal"), "new-hash", "buyer").unwrap();
        assert_eq!(change.approval_subject_version, 2);
        assert!(change.save_draft_target(&draft, &lines, None, "invalid".into(), "buyer").is_err());
    }

    // 当前审批提交必须同时满足当前指针、所属变更单、基准版本和待处理状态。
    #[test]
    fn current_approval_submission_refuses_history_and_foreign_identity() {
        let mut change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "采购调整".into(),
            },
            "buyer",
        )
        .unwrap();
        let mut submission = PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new("pcs-current"),
            change_submission_data(),
        )
        .unwrap();
        change.start_approval(PurchaseChangeSubmissionId::new("pcs-current"), "hash", "buyer").unwrap();
        assert!(change.ensure_current_approval_submission(&submission).is_err());

        submission.submit(Instant::from_unix_secs(1_700_000_000), "buyer").unwrap();
        change.ensure_current_approval_submission(&submission).unwrap();

        for invalid in [
            {
                let mut value = submission.clone();
                value.base.id = "history".into();
                value
            },
            {
                let mut value = submission.clone();
                value.purchase_change_order_id = PurchaseChangeOrderId::new("other");
                value
            },
            {
                let mut value = submission.clone();
                value.base_revision_id = PurchaseOrderRevisionId::new("other");
                value
            },
            {
                let mut value = submission.clone();
                value.approve().unwrap();
                value
            },
        ] {
            assert!(change.ensure_current_approval_submission(&invalid).is_err());
        }
    }

    #[test]
    fn change_submission_validates_triple_and_submits() {
        let submission =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-1"), change_submission_data())
                .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Draft);

        let inconsistent = PurchaseChangeSubmissionData {
            gross_amount: Amount::from_str("30.00").unwrap(),
            ..change_submission_data()
        };
        assert!(
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-2"), inconsistent).is_err()
        );

        let mut pending =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-3"), change_submission_data())
                .unwrap();
        pending.submit(Instant::from_unix_secs(1_700_000_000), "buyer-1").unwrap();
        assert_eq!(pending.status, SubmissionStatus::Pending);
        assert!(pending.submit(Instant::from_unix_secs(1_700_000_000), "buyer-1").is_err());
    }

    #[test]
    fn change_submission_derives_next_number_and_approves_only_pending() {
        let mut first =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-1"), change_submission_data())
                .unwrap();
        first.submission_no = "CS-000009".to_string();
        assert_eq!(
            PurchaseChangeSubmission::next_submission_no(std::slice::from_ref(&first)).unwrap(),
            "CS-000010"
        );

        assert!(first.approve().is_err());
        first.submit(Instant::from_unix_secs(1_700_000_000), "buyer-1").unwrap();
        first.approve().unwrap();
        assert_eq!(first.status, SubmissionStatus::Approved);
        assert!(first.approve().is_err());
    }
}
