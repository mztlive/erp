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
use crate::errors::{Error, Result};
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

/// 销售变更单状态（合同 §4.4.2：目标邻接仅草稿、审批中、已生效、已作废）。
///
/// 新写入只使用 `Draft` / `InApproval` / `Effective` / `Voided`。
/// `PendingImpactConfirmation`、`PendingFinanceReview` 与 `Rejected` 仅保留稳定
/// 字面量，供未改仓储查询编译；不得再作为目标状态机后继。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesChangeOrderStatus {
    /// 草稿。
    Draft,
    /// 审批中。
    InApproval,
    /// 已生效。
    Effective,
    /// 已作废。
    Voided,
    /// 残留字面量：原待影响确认，已合并入 `InApproval`。
    PendingImpactConfirmation,
    /// 残留字面量：原待财务复核，已合并入 `InApproval`。
    PendingFinanceReview,
    /// 残留字面量：原驳回态，审批驳回不再改业务状态。
    Rejected,
}

impl SalesChangeOrderStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::InApproval => "审批中",
            Self::Effective => "已生效",
            Self::Voided => "已作废",
            Self::PendingImpactConfirmation => "待影响确认",
            Self::PendingFinanceReview => "待财务复核",
            Self::Rejected => "已驳回",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::InApproval => "IN_APPROVAL",
            Self::Effective => "EFFECTIVE",
            Self::Voided => "VOIDED",
            Self::PendingImpactConfirmation => "PENDING_IMPACT_CONFIRMATION",
            Self::PendingFinanceReview => "PENDING_FINANCE_REVIEW",
            Self::Rejected => "REJECTED",
        }
    }
}

impl DocumentState for SalesChangeOrderStatus {
    /// 合同 §4.4.1 / §4.4.2：草稿可提交进入审批或作废；审批中可最终生效或
    /// 撤回回草稿；残留确认/驳回态无新后继。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::InApproval, Self::Voided],
            Self::InApproval => &[Self::Effective, Self::Draft],
            Self::Effective
            | Self::Voided
            | Self::PendingImpactConfirmation
            | Self::PendingFinanceReview
            | Self::Rejected => &[],
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
    /// 创建销售变更单（初始 `Draft`；目标提交在提交审批时形成）。
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

    /// 判断乐观锁版本是否与调用方期望一致。
    ///
    /// # 参数
    /// * `expected_version` - 调用方读取到的实体版本
    ///
    /// # 返回
    /// 当前版本与期望版本一致时返回 `true`。
    pub fn matches_version(&self, expected_version: u64) -> bool {
        self.base.version == expected_version
    }

    /// 判断变更单是否处于可提交或可作废的草稿态。
    ///
    /// # 返回
    /// 状态为 `Draft` 时返回 `true`。
    pub fn is_draft(&self) -> bool {
        self.stable.status == SalesChangeOrderStatus::Draft
    }

    /// 返回当前冻结的变更提交身份。
    ///
    /// # 返回
    /// 返回当前提交 ID。
    ///
    /// # 错误
    /// 变更单尚未提交审批时返回错误。
    pub fn required_current_submission_id(&self) -> Result<&SalesChangeSubmissionId> {
        self.current_submission_id
            .as_ref()
            .ok_or_else(|| Error::from("变更单尚未提交审批"))
    }

    /// 判断销售单当前版本是否仍等于变更基准版本。
    ///
    /// # 参数
    /// * `current_revision_id` - 原销售单当前版本
    ///
    /// # 返回
    /// 当前版本与变更单冻结基准版本一致时返回 `true`。
    pub fn base_revision_matches(&self, current_revision_id: &str) -> bool {
        self.base_revision_id.as_ref() == current_revision_id
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
        if self.stable.status != SalesChangeOrderStatus::Draft {
            return Err(crate::errors::Error::InvalidStateTransition {
                from: format!("{:?}", self.stable.status),
                to: format!("{:?}", SalesChangeOrderStatus::Draft),
            });
        }
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

    /// 提交并启动审批（`Draft → InApproval`）。
    ///
    /// 目标提交与内容指纹必须同时提供。`subject_version` 取提交序号，本方法
    /// 不回退已绑定的提交引用。
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
    /// 状态非法，或指纹为空、超长时返回错误。
    pub fn start_approval(
        &mut self,
        submission_id: SalesChangeSubmissionId,
        target_content_hash: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if self.stable.status != SalesChangeOrderStatus::Draft {
            return Err(crate::errors::Error::InvalidStateTransition {
                from: format!("{:?}", self.stable.status),
                to: format!("{:?}", SalesChangeOrderStatus::InApproval),
            });
        }
        ensure_transition(self.stable.status, SalesChangeOrderStatus::InApproval)?;
        let target_content_hash = normalize_required_text(
            target_content_hash.into(),
            "目标内容指纹不能为空",
            TARGET_HASH_MAX_LEN,
            "目标内容指纹过长",
        )?;
        self.current_submission_id = Some(submission_id);
        self.target_content_hash = Some(target_content_hash);
        self.stable.status = SalesChangeOrderStatus::InApproval;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 最终通过并生效（`InApproval → Effective`）。
    ///
    /// # 参数
    /// * `effective_revision_id` - 生效后生成的新销售版本
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非法时返回 [`Error::InvalidStateTransition`]。
    pub fn apply_effective(
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

    /// 撤回或受阻取消（`InApproval → Draft`）。
    ///
    /// 成功后回到可修正草稿；已冻结的提交引用与 `subject_version` 不回退。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审批中时返回 [`Error::InvalidStateTransition`]。
    pub fn cancel_approval(&mut self, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, SalesChangeOrderStatus::Draft)?;
        self.stable.status = SalesChangeOrderStatus::Draft;
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
    fn version_status_and_revision_relationship_rules_are_entity_owned() {
        let mut order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();
        assert!(order.matches_version(order.base.version));
        assert!(order.is_draft());
        assert!(order.base_revision_matches("rev-1"));
        assert!(order.required_current_submission_id().is_err());
        order
            .start_approval(SalesChangeSubmissionId::new("sub-1"), "hash-1", "admin-1")
            .unwrap();
        assert!(!order.is_draft());
        assert_eq!(order.required_current_submission_id().unwrap().as_ref(), "sub-1");
    }

    #[test]
    fn submit_enters_in_approval_and_cancel_returns_draft_without_rolling_back_submission() {
        let mut order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();
        order
            .start_approval(
                SalesChangeSubmissionId::new("cs-1"),
                " hash-1 ".to_string(),
                "admin-1",
            )
            .unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::InApproval);
        assert_eq!(order.target_content_hash.as_deref(), Some("hash-1"));
        assert_eq!(
            order.current_submission_id,
            Some(SalesChangeSubmissionId::new("cs-1"))
        );

        order.cancel_approval("admin-1").unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::Draft);
        assert_eq!(
            order.current_submission_id,
            Some(SalesChangeSubmissionId::new("cs-1")),
            "撤回不得回退 subject_version / 提交引用"
        );

        order
            .start_approval(SalesChangeSubmissionId::new("cs-2"), "hash-2", "admin-1")
            .unwrap();
        order
            .apply_effective(SalesOrderRevisionId::new("rev-2"), "finance")
            .unwrap();
        assert_eq!(order.stable.status(), SalesChangeOrderStatus::Effective);
        assert_eq!(
            order.effective_revision_id,
            Some(SalesOrderRevisionId::new("rev-2"))
        );
    }

    #[test]
    fn status_machine_edges_are_directed() {
        assert!(ensure_transition(SalesChangeOrderStatus::Draft, SalesChangeOrderStatus::InApproval).is_ok());
        assert!(ensure_transition(SalesChangeOrderStatus::Draft, SalesChangeOrderStatus::Voided).is_ok());
        assert!(ensure_transition(
            SalesChangeOrderStatus::InApproval,
            SalesChangeOrderStatus::Effective
        )
        .is_ok());
        assert!(ensure_transition(SalesChangeOrderStatus::InApproval, SalesChangeOrderStatus::Draft).is_ok());
        assert!(ensure_transition(SalesChangeOrderStatus::Draft, SalesChangeOrderStatus::Effective).is_err());
        assert!(ensure_transition(SalesChangeOrderStatus::Effective, SalesChangeOrderStatus::Draft).is_err());
        assert!(ensure_transition(SalesChangeOrderStatus::Voided, SalesChangeOrderStatus::Draft).is_err());
        assert!(SalesChangeOrderStatus::Effective.allowed_next().is_empty());
        assert!(SalesChangeOrderStatus::Voided.allowed_next().is_empty());
        assert!(SalesChangeOrderStatus::PendingImpactConfirmation
            .allowed_next()
            .is_empty());
        assert!(SalesChangeOrderStatus::PendingFinanceReview
            .allowed_next()
            .is_empty());
        assert!(SalesChangeOrderStatus::Rejected.allowed_next().is_empty());
        assert!(ensure_transition(
            SalesChangeOrderStatus::Draft,
            SalesChangeOrderStatus::PendingImpactConfirmation
        )
        .is_err());
        assert!(ensure_transition(
            SalesChangeOrderStatus::InApproval,
            SalesChangeOrderStatus::Rejected
        )
        .is_err());
    }

    #[test]
    fn start_approval_requires_paired_submission_and_hash() {
        let mut order = SalesChangeOrder::new(SalesChangeOrderId::new("co-1"), data(), "admin-1").unwrap();
        assert!(
            order
                .start_approval(SalesChangeSubmissionId::new("cs-1"), "  ", "admin-1")
                .is_err(),
            "内容指纹必填"
        );

        order
            .start_approval(SalesChangeSubmissionId::new("cs-1"), "hash-1", "admin-1")
            .unwrap();
        assert!(
            order
                .start_approval(SalesChangeSubmissionId::new("cs-2"), "hash-2", "admin-1")
                .is_err(),
            "审批中不可再次提交"
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
            .start_approval(SalesChangeSubmissionId::new("cs-1"), "hash-1", "admin-2")
            .unwrap();
        assert!(order
            .update(SalesChangeOrderUpdate::default(), "admin-3")
            .is_err());
    }
}
