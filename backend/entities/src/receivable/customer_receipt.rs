//! `customer_receipt` 客户回款单（数据模型 §6.8、§7.5 资金单据状态机）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CustomerAccountId, CustomerReceiptId, PartyId, ReceivableEntryId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 回款单号最大长度。
const RECEIPT_NO_MAX_LEN: usize = 64;
/// 银行流水引用最大长度。
const BANK_REFERENCE_MAX_LEN: usize = 256;
/// 创建人标识最大长度。
const CREATOR_ID_MAX_LEN: usize = 128;

/// 回款单状态（合同 §4.4.2：`PENDING_REVIEW` 收敛为 `IN_APPROVAL`，删除审批 `REJECTED`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerReceiptStatus {
    /// 草稿。
    Draft,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
    /// 已过账。
    Posted,
    /// 已冲正（存在正式反向事实，原事实不删除）。
    Reversed,
}

impl CustomerReceiptStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::InApproval => "审批中",
            Self::Posted => "已过账",
            Self::Reversed => "已冲正",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InApproval => "IN_APPROVAL",
            Self::Posted => "posted",
            Self::Reversed => "reversed",
        }
    }
}

impl DocumentState for CustomerReceiptStatus {
    /// 返回全部合法后继状态。
    ///
    /// 提交进入 `IN_APPROVAL`，最终通过过账，撤回回到草稿；`REVERSED` 是终态。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::InApproval],
            Self::InApproval => &[Self::Posted, Self::Draft],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 提交时冻结、待最终通过才落成正式核销的分配行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingReceiptAllocation {
    /// 被核销应收分录。
    pub receivable_entry_id: ReceivableEntryId,
    /// 本次核销金额（正数）。
    pub allocated_amount: Amount,
}

impl PendingReceiptAllocation {
    /// 构造待过账核销行。
    ///
    /// # 参数
    /// * `receivable_entry_id` - 被核销应收分录
    /// * `allocated_amount` - 核销金额
    ///
    /// # 错误
    /// 金额非正时返回错误。
    pub fn new(receivable_entry_id: ReceivableEntryId, allocated_amount: Amount) -> Result<Self> {
        ensure_positive_amount(&allocated_amount)?;
        Ok(Self {
            receivable_entry_id,
            allocated_amount,
        })
    }
}

/// 客户回款单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerReceiptData {
    /// 回款单号（唯一）。
    pub receipt_no: String,
    /// 实际付款往来主体。
    pub counterparty_party_id: PartyId,
    /// 可选经营归属提示（不参与核销相等判断）。
    pub customer_id: Option<CustomerAccountId>,
    /// 实际到账时间。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
}

/// 客户回款单更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CustomerReceiptUpdate {
    /// 到账时间；`None` 表示不修改。
    pub received_at: Option<Instant>,
    /// 到账金额；`None` 表示不修改。
    pub amount: Option<Amount>,
    /// 银行流水引用；`None` 表示不修改，`Some("")` 清除。
    pub bank_reference: Option<String>,
}

/// 客户回款单实体（正式事实，数据模型 §6.8）。
///
/// `receipt_no` 唯一；回款单往来主体必须等于应收子账往来主体、净分配合计不得
/// 超过已过账回款金额是跨实体约束，由 P3 分配事务校验（§8.3）。状态机见
/// §7.5：`POSTED` 后内容不可编辑，`REVERSED` 由 `receipt_reversal` 过账形成。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CustomerReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 创建人。旧数据缺失时反序列化为空，但不得据此执行绑定升级。
    #[serde(default)]
    pub created_by: String,
    /// 回款单状态。
    pub status: CustomerReceiptStatus,
    /// 回款单号。
    pub receipt_no: String,
    /// 实际付款往来主体。
    pub counterparty_party_id: PartyId,
    /// 可选经营归属提示。
    pub customer_id: Option<CustomerAccountId>,
    /// 实际到账时间。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
    /// 审批提交版本，初值 0。不得复用 `BaseModel.version`。
    #[serde(default)]
    pub approval_subject_version: u32,
    /// 提交时冻结的待过账核销分配。
    #[serde(default)]
    pub pending_allocations: Vec<PendingReceiptAllocation>,
}

/// 客户回款单审批事实快照（FIN-E09）。
///
/// 由 `CustomerReceipt` 唯一生成的稳定 approval facts：责任组织、对手方、
/// 金额与分配行数。提交人/提交时间由 Service 显式注入，不进入本结构；
/// 到 BPM action 的映射仍由 Service adapter 负责。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerReceiptApprovalFacts {
    /// 回款单号（`document_no` 稳定来源）。
    pub document_no: String,
    /// 责任组织（往来主体，非空）。
    pub responsible_org_id: String,
    /// 对手方客户（可选，不参与核销相等判断）。
    pub customer_id: Option<CustomerAccountId>,
    /// 含税到账金额。
    pub total_amount: Amount,
    /// 冻结核销分配行数。
    pub line_count: u32,
}

impl CustomerReceipt {
    /// 创建客户回款单（初始状态为草稿）。
    ///
    /// 完成回款单号与银行引用的 trim/非空/长度校验和金额正数校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CustomerReceiptId`）
    /// * `data` - 创建数据
    /// * `created_by` - 已认证创建人；创建后不得由更新命令覆盖
    ///
    /// # 返回
    /// 返回新建的回款单实体。
    ///
    /// # 错误
    /// 当回款单号或创建人为空/超长、银行引用超长或金额非正时返回错误。
    pub fn new(
        id: CustomerReceiptId,
        data: CustomerReceiptData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let receipt_no = normalize_required_text(
            data.receipt_no,
            "回款单号不能为空",
            RECEIPT_NO_MAX_LEN,
            "回款单号过长",
        )?;
        let bank_reference =
            normalize_optional_text(data.bank_reference, "银行流水引用", BANK_REFERENCE_MAX_LEN)?;
        let created_by = normalize_required_text(
            created_by.into(),
            "创建人不能为空",
            CREATOR_ID_MAX_LEN,
            "创建人标识过长",
        )?;
        ensure_positive_amount(&data.amount)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            created_by,
            status: CustomerReceiptStatus::Draft,
            receipt_no,
            counterparty_party_id: data.counterparty_party_id,
            customer_id: data.customer_id,
            received_at: data.received_at,
            amount: data.amount,
            bank_reference,
            approval_subject_version: 0,
            pending_allocations: Vec::new(),
        })
    }

    /// 返回审批责任组织（FIN-E09）。
    ///
    /// 取往来主体，不得用空串或当前登录人组织补位。
    ///
    /// # 返回
    /// 返回非空责任组织。
    ///
    /// # 错误
    /// 往来主体为空时返回错误。
    pub fn approval_responsible_org_id(&self) -> Result<String> {
        let org = self.counterparty_party_id.to_string();
        if org.trim().is_empty() {
            return Err(Error::from("客户回款单缺少往来主体，无法冻结责任组织"));
        }
        Ok(org)
    }

    /// 生成稳定的审批事实快照（FIN-E09）。
    ///
    /// 同一实体重复生成结果确定；空分配、空组织与行数溢出失败关闭。
    ///
    /// # 返回
    /// 返回文档号、责任组织、对手方、金额与行数事实。
    ///
    /// # 错误
    /// 无冻结分配、往来主体为空或行数溢出 `u32` 时返回错误。
    pub fn approval_facts(&self) -> Result<CustomerReceiptApprovalFacts> {
        if self.pending_allocations.is_empty() {
            return Err(Error::from("客户回款单没有核销分配，无法启动审批"));
        }
        Ok(CustomerReceiptApprovalFacts {
            document_no: self.receipt_no.clone(),
            responsible_org_id: self.approval_responsible_org_id()?,
            customer_id: self.customer_id.clone(),
            total_amount: self.amount,
            line_count: u32::try_from(self.pending_allocations.len())
                .map_err(|_| Error::from("回款核销分配行数溢出"))?,
        })
    }

    /// 校验客户回款仍是从未提交审批的初始草稿。
    ///
    /// # 错误
    /// 非草稿、审批主题版本已递增或已经冻结待过账分配时返回错误。
    pub fn ensure_initial_approval_state(&self) -> Result<()> {
        if self.status != CustomerReceiptStatus::Draft
            || self.approval_subject_version != 0
            || !self.pending_allocations.is_empty()
        {
            return Err(Error::from("客户回款单已经提交或启动过审批"));
        }
        Ok(())
    }

    /// 更新回款单草稿。
    ///
    /// 复用 `new` 的校验规则；`POSTED` 后内容不可编辑（§7.5），草稿修改不改变
    /// 回款单号与往来主体等关键字段。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: CustomerReceiptUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(amount) = update.amount {
            ensure_positive_amount(&amount)?;
            self.amount = amount;
        }
        if let Some(received_at) = update.received_at {
            self.received_at = received_at;
        }
        if let Some(bank_reference) = update.bank_reference {
            self.bank_reference =
                normalize_optional_text(Some(bank_reference), "银行流水引用", BANK_REFERENCE_MAX_LEN)?;
        }
        Ok(())
    }

    /// 迁移回款单状态。
    ///
    /// 按 §7.5 固定邻接矩阵校验并应用状态迁移。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标状态不在邻接矩阵中时返回 [`Error::InvalidStateTransition`]。
    pub fn transition(&mut self, to: CustomerReceiptStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }

    /// 提交并启动审批：递增 `approval_subject_version` 并进入 `IN_APPROVAL`。
    ///
    /// 版本使用 checked add，成功后不回退。不得改写 `BaseModel.version`。
    ///
    /// # 参数
    /// * `allocations` - 提交时冻结的待过账核销分配
    ///
    /// # 返回
    /// 返回冻结后的提交版本。
    ///
    /// # 错误
    /// 非草稿、分配非法或版本溢出时返回冲突。
    pub fn start_approval(&mut self, allocations: Vec<PendingReceiptAllocation>) -> Result<u32> {
        if self.status != CustomerReceiptStatus::Draft {
            return Err(Error::from("只有草稿状态的客户回款单可以提交审批"));
        }
        ensure_pending_allocations(&self.amount, &allocations)?;
        let next = self
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::from("审批提交版本溢出"))?;
        self.approval_subject_version = next;
        self.pending_allocations = allocations;
        self.transition(CustomerReceiptStatus::InApproval)?;
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self) -> Result<()> {
        if self.status != CustomerReceiptStatus::InApproval {
            return Err(Error::from("只有审批中的客户回款单可以撤回审批"));
        }
        self.transition(CustomerReceiptStatus::Draft)
    }

    /// 最终通过过账：仅 `IN_APPROVAL` 可进入 `POSTED`。
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn mark_posted(&mut self) -> Result<()> {
        if self.status != CustomerReceiptStatus::InApproval {
            return Err(Error::from("只有审批中的客户回款单可以由最终通过动作过账"));
        }
        self.transition(CustomerReceiptStatus::Posted)
    }

    /// 将经 W13 当前责任任务核验的历史回款登记为已过账事实。
    ///
    /// 本入口只表达历史事实迁移；调用方必须在同一数据库事务内校验开放的
    /// 卡券票款复核任务、当前责任、对象版本与分配守恒，并同步写入分配、
    /// 子账进度和审计。普通客户回款不得调用本入口绕过审批。
    ///
    /// # 错误
    /// 状态不是草稿时返回冲突。
    pub fn register_historical_fact(&mut self) -> Result<()> {
        if self.status != CustomerReceiptStatus::Draft {
            return Err(Error::from("只有草稿回款可以登记为历史事实"));
        }
        self.transition(CustomerReceiptStatus::InApproval)?;
        self.transition(CustomerReceiptStatus::Posted)
    }

    /// 判断回款单是否已过账。
    ///
    /// # 返回
    /// 状态为 `Posted` 时返回 `true`。
    pub fn is_posted(&self) -> bool {
        self.status == CustomerReceiptStatus::Posted
    }

    /// 校验回款单仍可编辑。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非草稿时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if self.status != CustomerReceiptStatus::Draft {
            return Err(Error::from("已过账或已冲正的回款单不可编辑"));
        }
        Ok(())
    }
}

/// 校验金额为正数。
///
/// # 错误
/// 零或负数时返回错误。
fn ensure_positive_amount(amount: &Amount) -> Result<()> {
    if amount.to_decimal().is_sign_negative() || amount.to_decimal().is_zero() {
        return Err(Error::from("回款金额必须为正数"));
    }
    Ok(())
}

/// 校验待过账分配合计不超过回款金额。
///
/// # 错误
/// 无分配行或合计超额时返回错误。
fn ensure_pending_allocations(amount: &Amount, allocations: &[PendingReceiptAllocation]) -> Result<()> {
    if allocations.is_empty() {
        return Err(Error::from("提交审批至少提供一条核销分配"));
    }
    let mut total = allocations[0].allocated_amount.to_decimal();
    for line in &allocations[1..] {
        total += line.allocated_amount.to_decimal();
    }
    if total > amount.to_decimal() {
        return Err(Error::from("核销合计超过回款金额"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> CustomerReceiptData {
        CustomerReceiptData {
            receipt_no: " RC-2026-001 ".to_string(),
            counterparty_party_id: PartyId::new("party-1"),
            customer_id: Some(CustomerAccountId::new("cust-1")),
            received_at: Instant::from_unix_secs(1_700_000_000),
            amount: Amount::from_str("1000.00").unwrap(),
            bank_reference: Some(" BANK-1 ".to_string()),
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let receipt = CustomerReceipt::new(CustomerReceiptId::new("cr-1"), data(), "creator-1").unwrap();

        assert_eq!(receipt.receipt_no, "RC-2026-001");
        assert_eq!(receipt.bank_reference.as_deref(), Some("BANK-1"));
        assert_eq!(receipt.status, CustomerReceiptStatus::Draft);
        assert!(!receipt.is_posted());
    }

    #[test]
    fn creator_is_normalized_and_legacy_missing_field_defaults_empty() {
        let mut receipt =
            CustomerReceipt::new(CustomerReceiptId::new("creator-cr"), data(), " creator-1 ").unwrap();
        receipt.update(CustomerReceiptUpdate::default()).unwrap();
        assert_eq!(receipt.created_by, "creator-1");
        assert!(CustomerReceipt::new(CustomerReceiptId::new("blank-creator-cr"), data(), "   ").is_err());

        let mut legacy = bson::serialize_to_document(&receipt).unwrap();
        legacy.remove("created_by");
        let legacy: CustomerReceipt = bson::deserialize_from_document(legacy).unwrap();
        assert!(legacy.created_by.is_empty());
    }

    #[test]
    fn new_rejects_blank_no_overlong_reference_and_non_positive() {
        let blank_no = CustomerReceiptData {
            receipt_no: "   ".to_string(),
            ..data()
        };
        assert!(CustomerReceipt::new(CustomerReceiptId::new("cr-2"), blank_no, "creator-1").is_err());

        let overlong = CustomerReceiptData {
            bank_reference: Some("b".repeat(257)),
            ..data()
        };
        assert!(CustomerReceipt::new(CustomerReceiptId::new("cr-3"), overlong, "creator-1").is_err());

        let non_positive = CustomerReceiptData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(CustomerReceipt::new(CustomerReceiptId::new("cr-4"), non_positive, "creator-1").is_err());
    }

    #[test]
    fn update_applies_draft_changes_and_rejects_posted() {
        let mut receipt = CustomerReceipt::new(CustomerReceiptId::new("cr-1"), data(), "creator-1").unwrap();

        receipt
            .update(CustomerReceiptUpdate {
                received_at: Some(Instant::from_unix_secs(1_700_000_100)),
                amount: Some(Amount::from_str("1200.00").unwrap()),
                bank_reference: Some(" BANK-2 ".to_string()),
            })
            .unwrap();
        assert_eq!(receipt.receipt_no, "RC-2026-001", "关键字段不改");
        assert_eq!(receipt.bank_reference.as_deref(), Some("BANK-2"));

        receipt
            .start_approval(vec![PendingReceiptAllocation::new(
                crate::ids::ReceivableEntryId::new("re-1"),
                Amount::from_str("100.00").unwrap(),
            )
            .unwrap()])
            .unwrap();
        receipt.mark_posted().unwrap();
        assert!(receipt.is_posted());
        assert!(receipt
            .update(CustomerReceiptUpdate {
                amount: Some(Amount::from_str("1.00").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn state_machine_edges_are_directed() {
        use crate::common::state::ensure_transition as tr;
        use CustomerReceiptStatus as S;

        assert!(tr(S::Draft, S::InApproval).is_ok());
        assert!(tr(S::InApproval, S::Posted).is_ok());
        assert!(tr(S::InApproval, S::Draft).is_ok());
        assert!(tr(S::Posted, S::Reversed).is_ok());
        assert!(tr(S::Reversed, S::Reversed).is_ok(), "幂等迁移恒合法");

        assert!(tr(S::Draft, S::Posted).is_err(), "草稿不得绕过审批直接过账");
        assert!(tr(S::Draft, S::Reversed).is_err());
        assert!(tr(S::Posted, S::Draft).is_err());
        assert!(tr(S::Posted, S::InApproval).is_err());
        assert!(tr(S::Reversed, S::Posted).is_err());
        assert!(tr(S::Reversed, S::Draft).is_err());

        let mut receipt = CustomerReceipt::new(CustomerReceiptId::new("cr-1"), data(), "creator-1").unwrap();
        assert!(receipt.transition(S::Reversed).is_err(), "实体迁移拒绝跨级");
    }

    #[test]
    fn start_approval_increments_version_and_cancel_does_not_rollback() {
        let mut receipt = CustomerReceipt::new(CustomerReceiptId::new("cr-1"), data(), "creator-1").unwrap();
        receipt
            .ensure_initial_approval_state()
            .expect("新建草稿是初始未提交状态");
        assert_eq!(receipt.approval_subject_version, 0);
        let version = receipt
            .start_approval(vec![PendingReceiptAllocation::new(
                crate::ids::ReceivableEntryId::new("re-1"),
                Amount::from_str("100.00").unwrap(),
            )
            .unwrap()])
            .unwrap();
        assert_eq!(version, 1);
        assert_eq!(receipt.status, CustomerReceiptStatus::InApproval);
        receipt.cancel_approval().unwrap();
        assert_eq!(receipt.status, CustomerReceiptStatus::Draft);
        assert_eq!(receipt.approval_subject_version, 1);
        assert!(receipt.ensure_initial_approval_state().is_err());
        assert!(receipt.mark_posted().is_err());
    }

    #[test]
    fn status_serializes_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&CustomerReceiptStatus::InApproval).unwrap(),
            "\"IN_APPROVAL\""
        );
        assert_eq!(CustomerReceiptStatus::Posted.label(), "已过账");
        assert_eq!(CustomerReceiptStatus::Reversed.as_str(), "reversed");
        assert_eq!(CustomerReceiptStatus::Draft.as_str(), "draft");
    }

    fn approved_receipt() -> CustomerReceipt {
        let mut receipt =
            CustomerReceipt::new(CustomerReceiptId::new("cr-approval"), data(), "creator-1").unwrap();
        receipt
            .start_approval(vec![PendingReceiptAllocation::new(
                crate::ids::ReceivableEntryId::new("re-1"),
                Amount::from_str("100.00").unwrap(),
            )
            .unwrap()])
            .unwrap();
        receipt
    }

    #[test]
    fn approval_facts_freeze_document_no_org_customer_amount_and_line_count() {
        let receipt = approved_receipt();
        let facts = receipt.approval_facts().unwrap();
        assert_eq!(facts.document_no, "RC-2026-001");
        assert_eq!(facts.responsible_org_id, "party-1");
        assert_eq!(
            facts.customer_id.as_ref().map(ToString::to_string),
            Some("cust-1".to_string())
        );
        assert_eq!(facts.total_amount, Amount::from_str("1000.00").unwrap());
        assert_eq!(facts.line_count, 1);
    }

    #[test]
    fn approval_facts_are_deterministic_across_repeated_generation() {
        let receipt = approved_receipt();
        assert_eq!(
            receipt.approval_facts().unwrap(),
            receipt.approval_facts().unwrap()
        );
    }

    #[test]
    fn approval_facts_allow_missing_customer_but_reject_empty_allocations() {
        let mut receipt =
            CustomerReceipt::new(CustomerReceiptId::new("cr-no-cust"), data(), "creator-1").unwrap();
        receipt.customer_id = None;
        receipt
            .start_approval(vec![PendingReceiptAllocation::new(
                crate::ids::ReceivableEntryId::new("re-1"),
                Amount::from_str("100.00").unwrap(),
            )
            .unwrap()])
            .unwrap();
        let facts = receipt.approval_facts().unwrap();
        assert!(facts.customer_id.is_none());
        assert_eq!(facts.document_no, "RC-2026-001");

        let draft = CustomerReceipt::new(CustomerReceiptId::new("cr-empty"), data(), "creator-1").unwrap();
        assert!(draft.approval_facts().is_err());
    }

    #[test]
    fn approval_responsible_org_rejects_blank_counterparty() {
        let mut receipt = approved_receipt();
        receipt.counterparty_party_id = PartyId::new("   ");
        assert!(receipt.approval_responsible_org_id().is_err());
        assert!(receipt.approval_facts().is_err());
    }
}
