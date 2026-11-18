//! 供应商资金原始证据，身份、金额事实与应用结果分别保留。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{DepositCurrency, DepositLedgerKind, SupplierDepositAccount, SupplierDepositLedgerEntry};

/// 原始响应只以受控引用、密文和内容摘要保存，禁止调试输出。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsEvidenceArchive {
    pub reference: String,
    pub body_encrypted: String,
    pub content_fingerprint: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FundsTransactionDirection {
    Recharge,
    Debit,
    Return,
}

/// 已解释交易与未解释观察分别保存；缺失事实不补造默认值。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FundsEvidenceResult {
    Transaction {
        stable_transaction_id: String,
        external_order_id: Option<String>,
        direction: FundsTransactionDirection,
        amount: Amount,
        currency: DepositCurrency,
        occurred_at: Instant,
        evidence_reference: String,
        original_transaction_id: Option<String>,
        bank_reference: Option<String>,
        business_code: Option<String>,
    },
    Unresolved {
        observation_id: String,
        evidence_reference: String,
        external_transaction_id: Option<String>,
        external_order_id: Option<String>,
        amount: Option<Amount>,
        occurred_at: Option<Instant>,
        business_code: Option<String>,
        reason_code: String,
    },
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsEvidenceInput {
    pub source_observed_at: Instant,
    pub source_revision: Option<String>,
    pub archive: FundsEvidenceArchive,
    pub result: FundsEvidenceResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FundsEvidenceKey {
    Transaction(String),
    Observation(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FundsEvidenceInboxState {
    Received,
    Applied,
    NeedsReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FundsAppliedResource {
    RechargeBatch { batch_id: String },
    DebitUsage { usage_id: String },
    ReturnUsage { usage_id: String },
}

/// 人工补齐真实资源后的重核回执，原来源和待核验原因继续保留。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsEvidenceReviewResolution {
    pub operation_key: String,
    pub payload_fingerprint: String,
    pub evidence_asset_id: String,
    pub actor_id: String,
    pub reviewed_at: Instant,
    pub original_reason_code: Option<String>,
}

/// 以账户和来源稳定身份去重的原始资金证据；冲突不能覆盖本行。
#[derive(Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct FundsEvidenceInbox {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub source_key: FundsEvidenceKey,
    pub semantic_fingerprint: String,
    pub input: FundsEvidenceInput,
    pub state: FundsEvidenceInboxState,
    pub first_observed_at: Instant,
    pub last_observed_at: Instant,
    pub applied_resource: Option<FundsAppliedResource>,
    pub review_reason_code: Option<String>,
    #[serde(default)]
    pub review_resolution: Option<FundsEvidenceReviewResolution>,
}

/// 同一来源身份的异载荷单独归档，原证据和已发生的应用结果均保留。
#[derive(Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct FundsEvidenceConflict {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub inbox_id: String,
    pub original_fingerprint: String,
    pub conflicting_fingerprint: String,
    pub conflicting_input: FundsEvidenceInput,
}

/// 余额采样独立于资金流水，不把不同采样时刻直接解释为账目差异。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsBalanceObservationData {
    pub observation_id: String,
    pub balance: Amount,
    pub currency: DepositCurrency,
    pub observed_at: Instant,
    pub source_revision: Option<String>,
    pub archive: FundsEvidenceArchive,
}

#[derive(Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct FundsBalanceObservation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub observation: FundsBalanceObservationData,
}

impl FundsEvidenceInput {
    /// 验证来源身份和脱敏字段，缺少金额的真实观察仍按未解释证据保存。
    /// # 参数
    /// 无。
    /// # 返回
    /// 字段有界且来源身份稳定时成功。
    /// # 错误
    /// 缺少稳定交易/观察身份、原归档依据或非法业务代码时拒绝。
    pub fn validate(&self) -> Result<()> {
        self.archive.validate()?;
        optional_identity(self.source_revision.as_deref())?;
        self.result.validate()
    }

    /// 返回带类型的来源身份，交易与未知观察不会相互误合并。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回原稳定身份。
    /// # 错误
    /// 身份非法时拒绝。
    pub fn key(&self) -> Result<FundsEvidenceKey> {
        match &self.result {
            FundsEvidenceResult::Transaction { stable_transaction_id, .. } => {
                identity(stable_transaction_id)?;
                Ok(FundsEvidenceKey::Transaction(stable_transaction_id.clone()))
            },
            FundsEvidenceResult::Unresolved { observation_id, .. } => {
                identity(observation_id)?;
                Ok(FundsEvidenceKey::Observation(observation_id.clone()))
            },
        }
    }

    pub(crate) fn semantic_result(&self) -> FundsEvidenceResult {
        let mut result = self.result.clone();
        match &mut result {
            FundsEvidenceResult::Transaction { evidence_reference, .. }
            | FundsEvidenceResult::Unresolved { evidence_reference, .. } => evidence_reference.clear(),
        }
        result
    }

    pub(crate) fn initial_state(&self) -> FundsEvidenceInboxState {
        match &self.result {
            FundsEvidenceResult::Transaction { amount, .. } if *amount > Amount::zero() => {
                FundsEvidenceInboxState::Received
            },
            _ => FundsEvidenceInboxState::NeedsReview,
        }
    }

    pub(crate) fn initial_review_code(&self) -> Option<String> {
        if self.initial_state() != FundsEvidenceInboxState::NeedsReview {
            return None;
        }
        match &self.result {
            FundsEvidenceResult::Unresolved { reason_code, .. } => Some(reason_code.clone()),
            _ => Some("INVALID_AMOUNT".into()),
        }
    }
}

impl FundsEvidenceArchive {
    pub(crate) fn validate(&self) -> Result<()> {
        identity(&self.reference)?;
        identity(&self.content_fingerprint)?;
        if self.body_encrypted.trim().is_empty() || self.body_encrypted.len() > 1_048_576 {
            return Err(Error::from("资金原始证据须有有界密文归档"));
        }
        Ok(())
    }
}

impl FundsEvidenceResult {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Transaction {
                stable_transaction_id,
                external_order_id,
                evidence_reference,
                original_transaction_id,
                bank_reference,
                business_code,
                ..
            } => {
                identity(stable_transaction_id)?;
                identity(evidence_reference)?;
                for value in [external_order_id, original_transaction_id, bank_reference] {
                    optional_identity(value.as_deref())?;
                }
                optional_code(business_code.as_deref())
            },
            Self::Unresolved {
                observation_id,
                evidence_reference,
                external_transaction_id,
                external_order_id,
                business_code,
                reason_code,
                ..
            } => {
                identity(observation_id)?;
                identity(evidence_reference)?;
                optional_identity(external_transaction_id.as_deref())?;
                optional_identity(external_order_id.as_deref())?;
                optional_code(business_code.as_deref())?;
                code(reason_code)
            },
        }
    }
}

impl FundsEvidenceInbox {
    /// 仅在服务已重验原真实资金资源后完成原待核验收件。
    /// # 参数
    /// 原方向对应的实际资源及带核验人、附件、操作身份的人工重核依据。
    /// # 返回
    /// 原证据和原核验原因保留，状态变为已应用。
    /// # 错误
    /// 来源冲突、未解释观察、资源改派或无完整人工依据时拒绝。
    pub fn resolve_reviewed(
        &mut self,
        resource: FundsAppliedResource,
        mut resolution: FundsEvidenceReviewResolution,
    ) -> Result<()> {
        for value in [
            &resolution.operation_key,
            &resolution.payload_fingerprint,
            &resolution.evidence_asset_id,
            &resolution.actor_id,
        ] {
            identity(value)?;
        }
        if self.state != FundsEvidenceInboxState::NeedsReview
            || self.review_reason_code.as_deref() == Some("SOURCE_FACT_CONFLICT")
            || self.applied_resource.as_ref().is_some_and(|previous| previous != &resource)
        {
            return Err(Error::from("原资金证据不是可重新匹配的待核验记录"));
        }

        let mut updated = self.clone();
        updated.state = FundsEvidenceInboxState::Received;
        updated.apply(resource)?;
        resolution.original_reason_code = self.review_reason_code.clone();
        updated.review_resolution = Some(resolution);
        *self = updated;
        Ok(())
    }

    /// 只有本次精确资金交易已可靠应用才允许结束对应消息线索。
    /// # 参数
    /// * `account` - 服务端已核验的原资金账户。
    /// * `transaction` / `order` / `expected` - 消息协议已核实的精确身份和方向。
    /// # 返回
    /// 本次交易已应用且无待核对事实时成功。
    /// # 错误
    /// 仅同单历史交易、未知结果、跨账户或资金仍待核对时拒绝。
    pub fn ensure_message_conclusion(
        &self,
        account: &SupplierDepositAccount,
        transaction: &str,
        order: &str,
        expected: FundsTransactionDirection,
    ) -> Result<()> {
        if account.base.id != self.account_id
            || account.reconciliation_required
            || self.state != FundsEvidenceInboxState::Applied
            || self.applied_resource.is_none()
            || (self.review_reason_code.is_some() && self.review_resolution.is_none())
            || self.source_key != FundsEvidenceKey::Transaction(transaction.into())
        {
            return Err(Error::from("本次消息资金交易尚未形成可追溯结论"));
        }

        match &self.input.result {
            FundsEvidenceResult::Transaction {
                stable_transaction_id, direction, external_order_id, ..
            } if stable_transaction_id == transaction
                && *direction == expected
                && external_order_id.as_deref() == Some(order) =>
            {
                Ok(())
            },
            _ => Err(Error::from("本次消息与资金交易、方向或订单不一致")),
        }
    }
    pub(crate) fn ensure_ledger_match(
        &self,
        ledger: &SupplierDepositLedgerEntry,
        kind: DepositLedgerKind,
    ) -> Result<()> {
        let FundsEvidenceResult::Transaction { stable_transaction_id, external_order_id, amount, .. } =
            &self.input.result
        else {
            return Err(Error::from("未解释观察不能匹配正式资金流水"));
        };
        if ledger.account_id != self.account_id
            || ledger.kind != kind
            || ledger.amount != *amount
            || ledger.evidence.external_transaction_id != *stable_transaction_id
            || external_order_id.as_ref().is_some_and(|order| order != &ledger.evidence.external_order_id)
        {
            return Err(Error::from("正式资金流水与原来源交易不一致"));
        }
        Ok(())
    }
    /// 保留原证据及原应用引用并转人工核对。
    /// # 参数
    /// * `reason_code` - 脱敏、稳定的业务原因代码。
    /// # 返回
    /// 状态变为待核对，原内容保持不变。
    /// # 错误
    /// 原因代码非法时拒绝且不修改。
    pub fn needs_review(&mut self, reason_code: &str) -> Result<()> {
        code(reason_code)?;
        self.state = FundsEvidenceInboxState::NeedsReview;
        self.review_reason_code = Some(reason_code.into());
        Ok(())
    }

    /// 校验应用结果与原交易方向一致；未解释或冲突证据不能自动完成。
    /// # 参数
    /// * `resource` - 同一事务已形成的真实财务资源。
    /// # 返回
    /// 新应用或相同资源重放成功。
    /// # 错误
    /// 未解释、待核对、改换资源或方向不符时拒绝且不修改。
    pub fn apply(&mut self, resource: FundsAppliedResource) -> Result<()> {
        if self.state == FundsEvidenceInboxState::Applied {
            if self.applied_resource.as_ref() == Some(&resource) {
                return Ok(());
            }
            return Err(Error::from("原资金证据已应用到另一资源"));
        }
        if self.state != FundsEvidenceInboxState::Received {
            return Err(Error::from("待核对资金证据不能自动应用"));
        }

        let direction = match &self.input.result {
            FundsEvidenceResult::Transaction { direction, amount, .. } if *amount > Amount::zero() => {
                *direction
            },
            _ => return Err(Error::from("资金观察尚未解释为稳定交易")),
        };
        let valid = match (&resource, direction) {
            (FundsAppliedResource::RechargeBatch { batch_id }, FundsTransactionDirection::Recharge) => {
                identity(batch_id)
            },
            (FundsAppliedResource::DebitUsage { usage_id }, FundsTransactionDirection::Debit)
            | (FundsAppliedResource::ReturnUsage { usage_id }, FundsTransactionDirection::Return) => {
                identity(usage_id)
            },
            _ => Err(Error::from("财务应用类型与原资金交易方向不一致")),
        };
        valid?;

        self.state = FundsEvidenceInboxState::Applied;
        self.applied_resource = Some(resource);
        Ok(())
    }
}

impl FundsBalanceObservationData {
    pub(crate) fn validate(&self) -> Result<()> {
        identity(&self.observation_id)?;
        optional_identity(self.source_revision.as_deref())?;
        self.archive.validate()
    }
}

pub(crate) fn identity(value: &str) -> Result<()> {
    if value.is_empty() || value != value.trim() || value.len() > 256 {
        return Err(Error::from("资金证据稳定身份或引用无效"));
    }
    Ok(())
}
pub(crate) fn optional_identity(value: Option<&str>) -> Result<()> {
    value.map(identity).transpose().map(|_| ())
}
pub(crate) fn code(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
    {
        return Err(Error::from("资金业务代码须为有界脱敏代码"));
    }
    Ok(())
}
fn optional_code(value: Option<&str>) -> Result<()> {
    value.map(code).transpose().map(|_| ())
}
