//! `sales_change_order`：销售变更单（数据模型 §6.5）。
//!
//! 变更单基于原销售单当前版本发起；生效前校验 `base_revision_id` 仍是当前版本
//! （防止并发覆盖，P3 事务校验）。每次修改拟变更内容都形成新的变更提交并使旧
//! 复核失效；变更生效事务把通过复核的结构化目标提交原样复制成新
//! `sales_order_revision` 及版本明细、追加应收差额，不改写旧版本（P3 服务职责）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::Result;
use crate::ids::{SalesChangeOrderId, SalesChangeSubmissionId, SalesOrderId, SalesOrderRevisionId};
use crate::validation::normalize_required_text;

/// 变更原因最大长度。
const REASON_MAX_LEN: usize = 512;
/// 目标内容指纹最大长度。
const TARGET_HASH_MAX_LEN: usize = 128;

/// 变更类型（数据模型 §6.5：商品、数量、金额、卡券类目、面额、期限等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesChangeType {
    /// 商品。
    Goods,
    /// 数量。
    Quantity,
    /// 金额。
    Amount,
    /// 卡券类目。
    VoucherCategory,
    /// 面额。
    FaceValue,
    /// 期限。
    ValidityPeriod,
    /// 其他。
    Other,
}

impl SalesChangeType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Goods => "商品",
            Self::Quantity => "数量",
            Self::Amount => "金额",
            Self::VoucherCategory => "卡券类目",
            Self::FaceValue => "面额",
            Self::ValidityPeriod => "期限",
            Self::Other => "其他",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Goods => "GOODS",
            Self::Quantity => "QUANTITY",
            Self::Amount => "AMOUNT",
            Self::VoucherCategory => "VOUCHER_CATEGORY",
            Self::FaceValue => "FACE_VALUE",
            Self::ValidityPeriod => "VALIDITY_PERIOD",
            Self::Other => "OTHER",
        }
    }
}

/// 销售变更单状态（数据模型 §6.5：草稿、待影响确认、待财务复核、已生效、已驳回、
/// 已作废）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesChangeOrderStatus {
    /// 草稿。
    Draft,
    /// 待影响确认（采购或运营的履约影响确认）。
    PendingImpactConfirmation,
    /// 待财务复核。
    PendingFinanceReview,
    /// 已生效。
    Effective,
    /// 已驳回。
    Rejected,
    /// 已作废。
    Voided,
    /// 审批中。
    InApproval,
}

impl SalesChangeOrderStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingImpactConfirmation => "待影响确认",
            Self::PendingFinanceReview => "待财务复核",
            Self::Effective => "已生效",
            Self::Rejected => "已驳回",
            Self::Voided => "已作废",
            Self::InApproval => "审批中",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingImpactConfirmation => "PENDING_IMPACT_CONFIRMATION",
            Self::PendingFinanceReview => "PENDING_FINANCE_REVIEW",
            Self::Effective => "EFFECTIVE",
            Self::Rejected => "REJECTED",
            Self::Voided => "VOIDED",
            Self::InApproval => "IN_APPROVAL",
        }
    }
}

impl DocumentState for SalesChangeOrderStatus {
    /// §6.5 固定邻接：草稿发起影响确认或作废；影响确认通过后进入财务复核；
    /// 财务复核通过后生效；影响确认/财务复核可驳回；驳回后修改内容形成新变更
    /// 提交并重新发起影响确认；生效/作废为终态。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingImpactConfirmation, Self::Voided, Self::InApproval],
            Self::InApproval => &[Self::Effective, Self::Draft],
            Self::PendingImpactConfirmation => &[Self::PendingFinanceReview, Self::Rejected],
            Self::PendingFinanceReview => &[Self::Effective, Self::Rejected],
            Self::Rejected => &[Self::PendingImpactConfirmation],
            Self::Effective | Self::Voided => &[],
        }
    }
}

/// 销售变更单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeOrderData {
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 发起时当前版本。
    pub base_revision_id: SalesOrderRevisionId,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更原因。
    pub reason: String,
}

/// 销售变更单更新数据（仅草稿态可修改变更类型与原因）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesChangeOrderUpdate {
    /// 变更类型；`None` 表示不修改。
    pub change_type: Option<SalesChangeType>,
    /// 变更原因；`None` 表示不修改。
    pub reason: Option<String>,
}

/// 销售变更单实体（数据模型 §6.5：同一销售单同一 `base_revision_id` 同时只能有
/// 一个进行中变更，唯一性由仓储/索引保证）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesChangeOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SalesChangeOrderStatus>,
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 发起时当前版本。
    pub base_revision_id: SalesOrderRevisionId,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更原因。
    pub reason: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<SalesChangeSubmissionId>,
    /// 目标完整内容指纹。
    pub target_content_hash: Option<String>,
    /// 生效后生成的新销售版本。
    pub effective_revision_id: Option<SalesOrderRevisionId>,
}

impl PartialEq for SalesChangeOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_order_id == other.sales_order_id
            && self.base_revision_id == other.base_revision_id
            && self.change_type == other.change_type
            && self.reason == other.reason
            && self.current_submission_id == other.current_submission_id
            && self.target_content_hash == other.target_content_hash
            && self.effective_revision_id == other.effective_revision_id
    }
}

impl Eq for SalesChangeOrder {}

impl SalesChangeOrder {
    /// 创建销售变更单（初始 `Draft`；目标提交在发起影响确认时形成）。
    ///
    /// 完成变更原因的校验与规范化（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesChangeOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人
    ///
    /// # 返回
    /// 返回新建的变更单实体。
    ///
    /// # 错误
    /// 变更原因为空或超长时返回错误。
    pub fn new(
        id: SalesChangeOrderId,
        data: SalesChangeOrderData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let reason =
            normalize_required_text(data.reason, "变更原因不能为空", REASON_MAX_LEN, "变更原因过长")?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(SalesChangeOrderStatus::Draft, created_by),
            sales_order_id: data.sales_order_id,
            base_revision_id: data.base_revision_id,
            change_type: data.change_type,
            reason,
            current_submission_id: None,
            target_content_hash: None,
            effective_revision_id: None,
        })
    }

    /// 更新变更单（仅 `Draft` 状态允许修改变更类型与原因）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非 `Draft`，或变更原因为空、超长时返回错误。
    pub fn update(&mut self, update: SalesChangeOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, SalesChangeOrderStatus::Draft)?;
        if let Some(change_type) = update.change_type {
            self.change_type = change_type;
        }
        if let Some(reason) = update.reason {
            self.reason =
                normalize_required_text(reason, "变更原因不能为空", REASON_MAX_LEN, "变更原因过长")?;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 发起影响确认（`Draft → PendingImpactConfirmation`）。
    ///
    /// 目标提交与内容指纹必须同时提供（§6.5：所有复核引用同一个
    /// `sales_change_submission_id`）。
    ///
    /// # 参数
    /// * `submission_id` - 不可变目标变更提交
    /// * `target_content_hash` - 目标完整内容指纹
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非法、提交与指纹缺一，或指纹为空、超长时返回错误。
    pub fn submit_impact(
        &mut self,
        submission_id: SalesChangeSubmissionId,
        target_content_hash: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        ensure_transition(
            self.stable.status,
            SalesChangeOrderStatus::PendingImpactConfirmation,
        )?;
        let target_content_hash = normalize_required_text(
            target_content_hash.into(),
            "目标内容指纹不能为空",
            TARGET_HASH_MAX_LEN,
            "目标内容指纹过长",
        )?;
        self.current_submission_id = Some(submission_id);
        self.target_content_hash = Some(target_content_hash);
        self.stable.status = SalesChangeOrderStatus::PendingImpactConfirmation;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 影响确认通过进入财务复核（`PendingImpactConfirmation → PendingFinanceReview`；
    /// 卡券变更完成运营确认后再做财务影响复核，§6.5）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待影响确认状态时返回 [`Error::InvalidStateTransition`]。
    pub fn to_finance_review(&mut self, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, SalesChangeOrderStatus::PendingFinanceReview)?;
        self.stable.status = SalesChangeOrderStatus::PendingFinanceReview;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 财务复核通过并生效（`PendingFinanceReview → Effective`）。
    ///
    /// # 参数
    /// * `effective_revision_id` - 生效后生成的新销售版本
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非法或缺少生效版本时返回错误。
    pub fn approve(
        &mut self,
        effective_revision_id: SalesOrderRevisionId,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        ensure_transition(self.stable.status, SalesChangeOrderStatus::Effective)?;
        self.effective_revision_id = Some(effective_revision_id);
        self.stable.status = SalesChangeOrderStatus::Effective;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 驳回变更（`PendingImpactConfirmation`/`PendingFinanceReview → Rejected`；
    /// 修改拟变更内容后形成新变更提交，从影响确认重新开始）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非法时返回 [`Error::InvalidStateTransition`]。
    pub fn reject(&mut self, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, SalesChangeOrderStatus::Rejected)?;
        self.stable.status = SalesChangeOrderStatus::Rejected;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 作废变更（`Draft → Voided`）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿态时返回 [`Error::InvalidStateTransition`]。
    pub fn void(&mut self, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, SalesChangeOrderStatus::Voided)?;
        self.stable.status = SalesChangeOrderStatus::Voided;
        self.stable.touch(updated_by);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SalesChangeOrderData {
        SalesChangeOrderData {
            sales_order_id: SalesOrderId::new("o-1"),
            base_revision_id: SalesOrderRevisionId::new("rev-1"),
            change_type: SalesChangeType::Quantity,
            reason: " 客户要求追加数量 ".to_string(),
        }
    }

    #[test]
    fn new_trims_reason_and_starts_draft() {
        let order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();

        assert_eq!(order.reason, "客户要求追加数量");
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::Draft);
        assert_eq!(order.change_type, SalesChangeType::Quantity);
        assert!(order.current_submission_id.is_none());
        assert!(order.effective_revision_id.is_none());
    }

    #[test]
    fn new_rejects_blank_and_overlong_reason() {
        let blank = SalesChangeOrderData {
            reason: "   ".to_string(),
            ..data()
        };
        assert!(SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), blank, "admin-1").is_err());

        let overlong = SalesChangeOrderData {
            reason: "x".repeat(513),
            ..data()
        };
        assert!(SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), overlong, "admin-1").is_err());
    }

    #[test]
    fn full_change_flow_and_rework_after_rejection() {
        let mut order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();
        order
            .submit_impact(
                SalesChangeSubmissionId::new("cs-1"),
                " hash-1 ".to_string(),
                "admin-1",
            )
            .unwrap();
        assert_eq!(
            order.stable.status(),
            SalesChangeOrderStatus::PendingImpactConfirmation
        );
        assert_eq!(order.target_content_hash.as_deref(), Some("hash-1"));

        order.reject("procurement").unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::Rejected);

        // 驳回后形成新变更提交，从影响确认重新开始（§6.5）
        order
            .submit_impact(SalesChangeSubmissionId::new("cs-2"), "hash-2", "admin-1")
            .unwrap();
        order.to_finance_review("operations").unwrap();
        assert_eq!(
            order.stable.status(),
            SalesChangeOrderStatus::PendingFinanceReview
        );
        order
            .approve(SalesOrderRevisionId::new("rev-2"), "finance")
            .unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::Effective);
        assert_eq!(
            order.effective_revision_id,
            Some(SalesOrderRevisionId::new("rev-2"))
        );
    }

    #[test]
    fn status_machine_edges_are_directed() {
        assert!(ensure_transition(
            SalesChangeOrderStatus::Draft,
            SalesChangeOrderStatus::PendingImpactConfirmation
        )
        .is_ok());
        assert!(ensure_transition(SalesChangeOrderStatus::Draft, SalesChangeOrderStatus::Voided).is_ok());
        assert!(ensure_transition(
            SalesChangeOrderStatus::PendingImpactConfirmation,
            SalesChangeOrderStatus::PendingFinanceReview
        )
        .is_ok());
        assert!(ensure_transition(
            SalesChangeOrderStatus::PendingImpactConfirmation,
            SalesChangeOrderStatus::Rejected
        )
        .is_ok());
        assert!(ensure_transition(
            SalesChangeOrderStatus::PendingFinanceReview,
            SalesChangeOrderStatus::Effective
        )
        .is_ok());
        assert!(ensure_transition(
            SalesChangeOrderStatus::PendingFinanceReview,
            SalesChangeOrderStatus::Rejected
        )
        .is_ok());
        assert!(ensure_transition(
            SalesChangeOrderStatus::Rejected,
            SalesChangeOrderStatus::PendingImpactConfirmation
        )
        .is_ok());
        // 非法迁移
        assert!(ensure_transition(SalesChangeOrderStatus::Draft, SalesChangeOrderStatus::Effective).is_err());
        assert!(ensure_transition(SalesChangeOrderStatus::Effective, SalesChangeOrderStatus::Draft).is_err());
        assert!(ensure_transition(SalesChangeOrderStatus::Voided, SalesChangeOrderStatus::Draft).is_err());
        assert!(SalesChangeOrderStatus::Effective.allowed_next().is_empty());
        assert!(SalesChangeOrderStatus::Voided.allowed_next().is_empty());
    }

    #[test]
    fn submit_impact_requires_paired_submission_and_hash() {
        let mut order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();
        assert!(
            order
                .submit_impact(SalesChangeSubmissionId::new("cs-1"), "  ", "admin-1")
                .is_err(),
            "内容指纹必填"
        );

        order
            .submit_impact(SalesChangeSubmissionId::new("cs-1"), "hash-1", "admin-1")
            .unwrap();
        order.to_finance_review("operations").unwrap();
        assert!(
            order
                .submit_impact(SalesChangeSubmissionId::new("cs-2"), "hash-2", "admin-1")
                .is_err(),
            "待财务复核不可重新发起影响确认"
        );
    }

    #[test]
    fn update_only_in_draft() {
        let mut order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();
        order
            .update(
                SalesChangeOrderUpdate {
                    change_type: Some(SalesChangeType::Amount),
                    reason: Some(" 调整金额 ".to_string()),
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.change_type, SalesChangeType::Amount);
        assert_eq!(order.reason, "调整金额");

        order
            .submit_impact(SalesChangeSubmissionId::new("cs-1"), "hash-1", "admin-2")
            .unwrap();
        assert!(order
            .update(SalesChangeOrderUpdate::default(), "admin-3")
            .is_err());
    }
}
