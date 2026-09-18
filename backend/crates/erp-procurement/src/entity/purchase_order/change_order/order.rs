//! `purchase_change_order` 采购变更单（数据模型 §6.6）。
//!
//! 采购变更单只适用于实物与服务销售单（phase-1 §6.3）；已入库、已付款和已形成
//! 发票的事实不回退，生效事务把已通过复核的目标提交原样复制为新采购版本、版本行
//! 和销售分配（§6.6 必需约束，P3 编排）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::stable::StableBase;
use erp_core::ids::{
    PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::types::status_display;

/// 变更原因最大长度。
const REASON_MAX_LEN: usize = 500;
/// 目标内容指纹最大长度。
const CONTENT_HASH_MAX_LEN: usize = 128;

/// 采购变更单状态（合同 §4.4.1 / §4.4.2：草稿、审批中、已生效、作废）。
///
/// 创建与提交均为 `Draft`，启动后 `InApproval`，最终通过 `Effective`。
/// `PENDING_WAREHOUSE_IMPACT`、`PENDING_FINANCE_REVIEW` 与审批导致的 `Rejected`
/// 已删除，节点事实只存在于审批实例。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseChangeOrderStatus {
    /// 草稿。
    Draft,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
    /// 已生效。
    Effective,
    /// 作废。
    Voided,
}

status_display!(PurchaseChangeOrderStatus, {
    Draft => ("草稿", "DRAFT"),
    InApproval => ("审批中", "IN_APPROVAL"),
    Effective => ("已生效", "EFFECTIVE"),
    Voided => ("作废", "VOIDED"),
});

impl PurchaseChangeOrderStatus {
    /// 判断状态是否代表尚未结束的采购变更。
    ///
    /// # 返回
    /// 草稿或审批中返回 `true`，已生效或作废返回 `false`。
    pub fn is_in_progress(self) -> bool {
        matches!(self, Self::Draft | Self::InApproval)
    }
}

/// 采购变更单创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderData {
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 采购变化原因。
    pub reason: String,
}

/// 采购变更单更新数据。
///
/// 内容编辑只允许在草稿状态（§7.4：生效后变化走变更单，变更单自身草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PurchaseChangeOrderUpdate {
    /// 采购变化原因；`None` 表示不修改。
    pub reason: Option<String>,
    /// 当前不可变目标提交；`None` 表示不修改。
    pub current_submission_id: Option<PurchaseChangeSubmissionId>,
    /// 目标提交内容指纹；`None` 表示不修改。
    pub target_content_hash: Option<String>,
    /// 生效后形成的新采购版本；`None` 表示不修改。
    pub effective_revision_id: Option<PurchaseOrderRevisionId>,
}

/// 采购变更单实体（可编辑单据草稿，数据模型 §6.6）。
///
/// `StableBase` 未派生 `PartialEq`，因此本实体手工实现全字段语义相等。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct PurchaseChangeOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<PurchaseChangeOrderStatus>,
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 采购变化原因。
    pub reason: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<PurchaseChangeSubmissionId>,
    /// 目标提交内容指纹。
    pub target_content_hash: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<PurchaseOrderRevisionId>,
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
}

impl PartialEq for PurchaseChangeOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.purchase_order_id == other.purchase_order_id
            && self.base_revision_id == other.base_revision_id
            && self.reason == other.reason
            && self.current_submission_id == other.current_submission_id
            && self.target_content_hash == other.target_content_hash
            && self.effective_revision_id == other.effective_revision_id
            && self.approval_subject_version == other.approval_subject_version
    }
}

impl Eq for PurchaseChangeOrder {}

impl PurchaseChangeOrder {
    /// 创建采购变更单。
    ///
    /// 完成变更原因校验与规范化；初始状态为 `Draft`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::PurchaseChangeOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的变更单实体。
    ///
    /// # 错误
    /// 变更原因为空或超长时返回错误。
    pub fn new(
        id: PurchaseChangeOrderId,
        data: PurchaseChangeOrderData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let reason =
            normalize_required_text(data.reason, "采购变化原因不能为空", REASON_MAX_LEN, "采购变化原因过长")?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(PurchaseChangeOrderStatus::Draft, created_by),
            purchase_order_id: data.purchase_order_id,
            base_revision_id: data.base_revision_id,
            reason,
            current_submission_id: None,
            target_content_hash: None,
            effective_revision_id: None,
            approval_subject_version: 0,
        })
    }

    /// 校验调用方持有的乐观锁版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 期望版本与实体当前版本不一致时返回领域错误。
    pub fn ensure_expected_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("采购变更单版本已变化"));
        }
        Ok(())
    }

    /// 校验变更单仍可冻结新的审批提交。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿状态返回领域错误。
    pub fn ensure_draft_for_submission(&self) -> Result<()> {
        if self.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::from("变更单已提交，请勿重复提交"));
        }
        Ok(())
    }

    /// 解析最终生效动作必须使用的当前冻结提交。
    ///
    /// # 参数
    /// * `requested` - 可选的调用方提交 ID；空值表示直接采用当前冻结提交
    ///
    /// # 返回
    /// 返回当前冻结提交的类型化稳定身份。
    ///
    /// # 错误
    /// 变更单尚未提交，或请求提交与当前冻结提交不一致时返回领域错误。
    pub fn submission_id_for_effect(&self, requested: Option<&str>) -> Result<PurchaseChangeSubmissionId> {
        let current = self.current_submission_id.clone().ok_or_else(|| Error::from("变更单尚未提交审批"))?;
        if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty())
            && requested != current.as_ref()
        {
            return Err(Error::from("生效提交必须是当前冻结提交，不得使用历史提交"));
        }
        Ok(current)
    }

    /// 校验变更基准版本仍是采购单当前生效版本。
    ///
    /// # 参数
    /// * `current_revision_id` - 原采购单当前生效版本
    ///
    /// # 返回
    /// 基准版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 原采购单缺少当前版本，或当前版本已偏离变更基准时返回领域错误。
    pub fn ensure_base_revision_current(&self, current_revision_id: Option<&str>) -> Result<()> {
        if current_revision_id != Some(self.base_revision_id.as_ref()) {
            return Err(Error::from("基准版本已不是当前版本，变更不能生效"));
        }
        Ok(())
    }

    /// 更新采购变更单。
    ///
    /// 原因/目标提交等内容只允许在草稿状态编辑；状态与生效版本由
    /// P3 按 §6.6/§8.1 第 3 条编排（`purchase_order_id`、`base_revision_id` 不可修改）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿，或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: PurchaseChangeOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_content(&update, updated_by)
    }

    /// 应用内容更新（草稿门禁）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 错误
    /// 状态不是草稿，或原因/内容指纹校验失败时返回错误。
    fn apply_content(
        &mut self,
        update: &PurchaseChangeOrderUpdate,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if update_has_content(update) && self.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::from("只有草稿状态的采购变更单可以编辑内容"));
        }
        if let Some(reason) = update.reason.clone() {
            self.reason =
                normalize_required_text(reason, "采购变化原因不能为空", REASON_MAX_LEN, "采购变化原因过长")?;
        }
        if let Some(submission_id) = update.current_submission_id.clone() {
            self.current_submission_id = Some(submission_id);
        }
        if let Some(hash) = update.target_content_hash.clone() {
            self.target_content_hash = Some(normalize_required_text(
                hash,
                "目标内容指纹不能为空",
                CONTENT_HASH_MAX_LEN,
                "目标内容指纹过长",
            )?);
        }
        if let Some(revision_id) = update.effective_revision_id.clone() {
            self.effective_revision_id = Some(revision_id);
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 提交并启动审批：递增 `approval_subject_version` 并进入 `IN_APPROVAL`。
    ///
    /// 版本使用 checked add，成功后不回退。不得改写 `BaseModel.version`。
    ///
    /// # 参数
    /// * `submission_id` - 本次冻结的不可变目标提交
    /// * `target_content_hash` - 目标内容指纹
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 返回冻结后的提交版本。
    ///
    /// # 错误
    /// 非草稿、指纹非法或版本溢出时返回冲突。
    pub fn start_approval(
        &mut self,
        submission_id: PurchaseChangeSubmissionId,
        target_content_hash: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<u32> {
        if self.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::from("只有草稿状态的采购变更单可以提交审批"));
        }
        let next =
            self.approval_subject_version.checked_add(1).ok_or_else(|| Error::from("审批提交版本溢出"))?;
        let target_content_hash = normalize_required_text(
            target_content_hash.into(),
            "目标内容指纹不能为空",
            CONTENT_HASH_MAX_LEN,
            "目标内容指纹过长",
        )?;
        self.approval_subject_version = next;
        self.current_submission_id = Some(submission_id);
        self.target_content_hash = Some(target_content_hash);
        self.stable.status = PurchaseChangeOrderStatus::InApproval;
        self.stable.touch(updated_by);
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 参数
    /// * `updated_by` - 撤回人
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status != PurchaseChangeOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购变更单可以撤回审批"));
        }
        self.stable.status = PurchaseChangeOrderStatus::Draft;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 最终通过并生效：仅 `IN_APPROVAL` 可进入 `EFFECTIVE`。
    ///
    /// # 参数
    /// * `effective_revision_id` - 生效后形成的新采购版本
    /// * `updated_by` - 最终通过执行人
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn apply_effective(
        &mut self,
        effective_revision_id: PurchaseOrderRevisionId,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if self.stable.status != PurchaseChangeOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购变更单可以由最终通过动作生效"));
        }
        self.effective_revision_id = Some(effective_revision_id);
        self.stable.status = PurchaseChangeOrderStatus::Effective;
        self.stable.touch(updated_by);
        Ok(())
    }
}

/// 判断更新数据是否包含内容字段。
///
/// # 参数
/// * `update` - 更新数据
///
/// # 返回
/// 包含原因/目标提交/内容指纹/生效版本任一字段时返回 `true`。
fn update_has_content(update: &PurchaseChangeOrderUpdate) -> bool {
    update.reason.is_some()
        || update.current_submission_id.is_some()
        || update.target_content_hash.is_some()
        || update.effective_revision_id.is_some()
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{
        PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
    };

    use super::{
        PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate,
    };

    fn change_data() -> PurchaseChangeOrderData {
        PurchaseChangeOrderData {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            base_revision_id: PurchaseOrderRevisionId::new("por-1"),
            reason: " 成本上涨调整 ".to_string(),
        }
    }

    #[test]
    fn change_order_new_trims_reason_and_starts_draft() {
        let order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        assert_eq!(order.reason, "成本上涨调整");
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(order.approval_subject_version, 0);
        assert!(order.current_submission_id.is_none());
    }

    #[test]
    fn change_order_version_submission_and_base_revision_guards_are_owned_by_entity() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        order.ensure_expected_version(order.base.version).unwrap();
        assert!(order.ensure_expected_version(order.base.version.saturating_add(1)).is_err());
        order.ensure_draft_for_submission().unwrap();
        assert!(order.submission_id_for_effect(None).is_err());
        order.start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "admin-1").unwrap();
        assert_eq!(order.submission_id_for_effect(Some("pcs-1")).unwrap().as_ref(), "pcs-1");
        assert!(order.submission_id_for_effect(Some("pcs-old")).is_err());
        order.ensure_base_revision_current(Some("por-1")).unwrap();
        assert!(order.ensure_base_revision_current(Some("por-2")).is_err());
    }

    #[test]
    fn change_order_update_gates_content_on_draft() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        order
            .update(
                PurchaseChangeOrderUpdate {
                    reason: Some("价格下降".to_string()),
                    current_submission_id: Some(PurchaseChangeSubmissionId::new("pcs-1")),
                    target_content_hash: Some("hash-1".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.reason, "价格下降");
        assert_eq!(order.stable.updated_by, "admin-2");

        order.start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "admin-2").unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::InApproval);
        assert_eq!(order.approval_subject_version, 1);

        assert!(
            order
                .update(
                    PurchaseChangeOrderUpdate { reason: Some("再改".to_string()), ..Default::default() },
                    "admin-3",
                )
                .is_err(),
            "非草稿不得编辑内容"
        );
    }

    /// 提交进入审批中；撤回不回退版本；最终通过进入生效。
    #[test]
    fn start_approval_cancel_and_apply_effective() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        let version =
            order.start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "submitter-1").unwrap();
        assert_eq!(version, 1);
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::InApproval);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(order.stable.updated_by, "submitter-1");

        order.cancel_approval("admin-2").unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(order.current_submission_id.as_ref().map(ToString::to_string).as_deref(), Some("pcs-1"));

        let next =
            order.start_approval(PurchaseChangeSubmissionId::new("pcs-2"), "hash-2", "submitter-2").unwrap();
        assert_eq!(next, 2);
        order.apply_effective(PurchaseOrderRevisionId::new("por-2"), "approver-1").unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Effective);
        assert_eq!(order.approval_subject_version, 2);
        assert!(order.start_approval(PurchaseChangeSubmissionId::new("pcs-3"), "h", "u").is_err());
        assert!(order.cancel_approval("u").is_err());
        assert!(order.apply_effective(PurchaseOrderRevisionId::new("por-3"), "u").is_err());
    }

    /// 内容更新不得改写状态；状态只能经签署邻接方法迁移。
    #[test]
    fn update_cannot_rewrite_status() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        order
            .update(
                PurchaseChangeOrderUpdate { reason: Some("仅改原因".to_string()), ..Default::default() },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(order.reason, "仅改原因");
    }

    #[test]
    fn change_order_new_rejects_empty_reason() {
        let data = PurchaseChangeOrderData { reason: "   ".to_string(), ..change_data() };
        assert!(PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-2"), data, "admin-1").is_err());
    }
}
