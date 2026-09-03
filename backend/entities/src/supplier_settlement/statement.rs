//! `supplier_settlement_statement`（数据模型 §6.20 供应商周期结算单）。
//!
//! 结算单是正式单据：`statement_no`、供应商、结算期间与外部账单身份创建后不可修改；
//! 经办人与复核人不得相同；`difference_amount` 由双方金额派生并强制恒等；已确认状态
//! 必须携带确认时间与应付账户，已作废为终态。

use std::collections::HashMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{PayableAccountId, SupplierAccountId, SupplierSettlementStatementId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::difference::{SettlementDifferenceStatus, SupplierSettlementDifference};
use super::item::{SettlementCostDelta, SupplierSettlementItem};
use super::review_reason::SettlementReviewRejectReason;

/// 结算单号最大长度。
const STATEMENT_NO_MAX_LEN: usize = 64;
/// 外部账单号/版本最大长度。
const EXTERNAL_BILL_NO_MAX_LEN: usize = 64;
/// 经办人/复核人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;
/// SHA-256 十六进制摘要长度。
const HASH_LEN: usize = 64;
/// 冻结策略标识/版本最大长度。
const POLICY_VALUE_MAX_LEN: usize = 128;
/// 复核说明最大长度。
const REVIEW_COMMENT_MAX_LEN: usize = 512;

/// 结算复核的正式决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewResult {
    /// 复核确认并形成应付。
    Confirmed,
    /// 驳回给经办人继续处理。
    Rejected,
}

/// 结算复核正式决定数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettlementReviewDecision {
    /// 确认结算。
    Confirm {
        /// 同事务形成的应付账户。
        payable_account_id: PayableAccountId,
        /// 复核说明。
        comment: Option<String>,
    },
    /// 驳回复核。
    Reject {
        /// 驳回后的可编辑状态，只允许草稿或有差异。
        return_status: SettlementStatus,
        /// 结构化驳回原因（固定三元值对象，绕过 Service 也不能写入未知原因）。
        reason_code: SettlementReviewRejectReason,
        /// 补充说明。
        comment: Option<String>,
    },
}

/// 结算单状态（数据模型 §6.20：草稿、待对账、有差异、待复核、已确认、已作废）。
///
/// 固定枚举（§4.6），不属于数据模型第 7 章的固定状态机；结算确认编排（§8.4 第 6 条）
/// 由 P3 承担。实体层固化保守守卫：已作废为终态，已确认只能作废。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementStatus {
    /// 草稿。
    Draft,
    /// 待对账。
    PendingReconciliation,
    /// 有差异。
    HasDifference,
    /// 待复核。
    PendingReview,
    /// 已确认：同事务形成应付（§8.4 第 6 条，P3）。
    Confirmed,
    /// 已作废：终态。
    Voided,
}

impl SettlementStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingReconciliation => "待对账",
            Self::HasDifference => "有差异",
            Self::PendingReview => "待复核",
            Self::Confirmed => "已确认",
            Self::Voided => "已作废",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingReconciliation => "PENDING_RECONCILIATION",
            Self::HasDifference => "HAS_DIFFERENCE",
            Self::PendingReview => "PENDING_REVIEW",
            Self::Confirmed => "CONFIRMED",
            Self::Voided => "VOIDED",
        }
    }
}

/// 结算单创建数据（不含系统字段；`difference_amount` 由双方金额派生）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementData {
    /// ERP 结算单号（唯一）。
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 供应商结算期间策略。
    pub period_policy_id: String,
    /// 供应商结算期间策略冻结版本。
    pub period_policy_version: String,
    /// 供应商结算期间策略时区。
    pub period_timezone: String,
    /// 供应商账单号，可空（与版本成对出现）。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本，可空。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: Amount,
    /// 供应商金额。
    pub supplier_amount: Amount,
    /// 覆盖冻结来源、结算明细和差异结论的主题摘要。
    pub subject_hash: String,
    /// 正式来源事实水位。
    pub source_as_of: Instant,
    /// 来源快照冻结时间。
    pub source_snapshot_at: Instant,
    /// 不可变来源快照摘要。
    pub source_snapshot_hash: String,
    /// 提交复核时采用的刷新截止策略。
    pub refresh_cutoff_policy_id: String,
    /// 刷新截止策略冻结版本。
    pub refresh_cutoff_policy_version: String,
    /// 经办人。
    pub prepared_by: String,
}

/// 草稿刷新所替换的服务端来源快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSnapshotUpdate {
    pub external_bill_no: String,
    pub external_bill_version: String,
    pub erp_amount: Amount,
    pub supplier_amount: Amount,
    pub source_as_of: Instant,
    pub source_snapshot_at: Instant,
    pub source_snapshot_hash: String,
    pub has_difference: bool,
}

/// 结算单更新数据（不含系统字段与关键字段）。
///
/// 单号、供应商、结算期间与外部账单身份创建后不可修改；金额变更会按恒等式重算
/// `difference_amount`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierSettlementStatementUpdate {
    /// ERP 金额；`None` 表示不修改。
    pub erp_amount: Option<Amount>,
    /// 供应商金额；`None` 表示不修改。
    pub supplier_amount: Option<Amount>,
    /// 结算状态；`None` 表示不修改。
    pub status: Option<SettlementStatus>,
    /// 复核人；`None` 表示不修改。
    pub reviewed_by: Option<String>,
    /// 应付账户（状态推进到已确认时必填）。
    pub payable_account_id: Option<PayableAccountId>,
}

/// 供应商周期结算单实体（数据模型 §6.20，正式单据）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementStatement {
    #[serde(flatten)]
    pub base: BaseModel,
    /// ERP 结算单号。
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 冻结的供应商结算期间策略。
    pub period_policy_id: String,
    /// 冻结的供应商结算期间策略版本。
    pub period_policy_version: String,
    /// 冻结的供应商结算期间策略时区。
    pub period_timezone: String,
    /// 供应商账单号。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: Amount,
    /// 供应商金额。
    pub supplier_amount: Amount,
    /// 双方金额差异（= 供应商金额 − ERP 金额）。
    pub difference_amount: Amount,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 覆盖冻结来源、结算明细和差异结论的主题摘要。
    pub subject_hash: String,
    /// 正式来源事实水位。
    pub source_as_of: Instant,
    /// 来源快照冻结时间。
    pub source_snapshot_at: Instant,
    /// 不可变来源快照摘要。
    pub source_snapshot_hash: String,
    /// 提交复核时采用的刷新截止策略。
    pub refresh_cutoff_policy_id: String,
    /// 刷新截止策略冻结版本。
    pub refresh_cutoff_policy_version: String,
    /// 经办人。
    pub prepared_by: String,
    /// 复核人。
    pub reviewed_by: Option<String>,
    /// 最近一次正式复核决定。
    pub review_result: Option<SettlementReviewResult>,
    /// 最近一次驳回原因代码；确认时为空。
    pub review_reason_code: Option<String>,
    /// 最近一次复核说明。
    pub review_comment: Option<String>,
    /// 最近一次复核决定时间。
    pub reviewed_at: Option<Instant>,
    /// 确认时间。
    pub confirmed_at: Option<Instant>,
    /// 确认后形成的应付账户。
    pub payable_account_id: Option<PayableAccountId>,
}

impl SupplierSettlementStatement {
    /// 创建供应商周期结算单。
    ///
    /// 完成单号、外部账单身份与经办/复核人的校验和规范化，并强制四条不变式：
    /// 期间结束不早于开始；外部账单号与版本成对出现；双方金额非负且
    /// `difference_amount = supplier_amount − erp_amount` 恒等；已确认状态必须携带
    /// 确认时间与应付账户且两者成对。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierSettlementStatementId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的结算单实体。
    ///
    /// # 错误
    /// 单号为空/超长、期间倒挂、账单身份不完整、经办复核相同、金额为负或
    /// 确认状态字段不一致时返回错误。
    pub fn new(id: SupplierSettlementStatementId, data: SupplierSettlementStatementData) -> Result<Self> {
        let statement_no = normalize_required_text(
            data.statement_no,
            "结算单号不能为空",
            STATEMENT_NO_MAX_LEN,
            "结算单号过长",
        )?;
        let external_bill_no =
            normalize_optional_text(data.external_bill_no, "外部账单号", EXTERNAL_BILL_NO_MAX_LEN)?;
        let external_bill_version = normalize_optional_text(
            data.external_bill_version,
            "外部账单版本",
            EXTERNAL_BILL_NO_MAX_LEN,
        )?;
        if external_bill_no.is_some() != external_bill_version.is_some() {
            return Err(Error::from("外部账单号与版本必须同时提供或同时省略"));
        }
        let prepared_by =
            normalize_required_text(data.prepared_by, "经办人不能为空", ACTOR_MAX_LEN, "经办人过长")?;
        if data.period_end < data.period_start {
            return Err(Error::from("结算期间结束不得早于开始"));
        }
        ensure_amount_non_negative(data.erp_amount, "ERP 结算金额不得为负")?;
        ensure_amount_non_negative(data.supplier_amount, "供应商结算金额不得为负")?;
        let subject_hash = normalize_sha256(data.subject_hash, "主题摘要")?;
        let source_snapshot_hash = normalize_sha256(data.source_snapshot_hash, "来源快照摘要")?;
        let period_policy_id = normalize_required_text(
            data.period_policy_id,
            "结算期间策略不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略过长",
        )?;
        let period_policy_version = normalize_required_text(
            data.period_policy_version,
            "结算期间策略版本不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略版本过长",
        )?;
        let period_timezone = normalize_required_text(
            data.period_timezone,
            "结算期间策略时区不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略时区过长",
        )?;
        let refresh_cutoff_policy_id = normalize_required_text(
            data.refresh_cutoff_policy_id,
            "刷新截止策略不能为空",
            POLICY_VALUE_MAX_LEN,
            "刷新截止策略过长",
        )?;
        let refresh_cutoff_policy_version = normalize_required_text(
            data.refresh_cutoff_policy_version,
            "刷新截止策略版本不能为空",
            POLICY_VALUE_MAX_LEN,
            "刷新截止策略版本过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            statement_no,
            supplier_id: data.supplier_id,
            period_start: data.period_start,
            period_end: data.period_end,
            period_policy_id,
            period_policy_version,
            period_timezone,
            external_bill_no,
            external_bill_version,
            erp_amount: data.erp_amount,
            supplier_amount: data.supplier_amount,
            difference_amount: data.supplier_amount.checked_sub(data.erp_amount),
            status: SettlementStatus::Draft,
            subject_hash,
            source_as_of: data.source_as_of,
            source_snapshot_at: data.source_snapshot_at,
            source_snapshot_hash,
            refresh_cutoff_policy_id,
            refresh_cutoff_policy_version,
            prepared_by,
            reviewed_by: None,
            review_result: None,
            review_reason_code: None,
            review_comment: None,
            reviewed_at: None,
            confirmed_at: None,
            payable_account_id: None,
        })
    }

    /// 校验调用方持有的结算单版本仍是当前版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的结算单版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本不一致时返回领域错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("供应商结算单版本不一致"));
        }
        Ok(())
    }

    /// 判断结算单是否仍处于可编辑草稿阶段。
    ///
    /// # 返回
    /// 草稿、待对账或有差异状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        matches!(
            self.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        )
    }

    /// 判断结算单是否已经作废。
    ///
    /// # 返回
    /// 状态为 `VOIDED` 时返回 `true`。
    pub fn is_voided(&self) -> bool {
        self.status == SettlementStatus::Voided
    }

    /// 判断结算单是否正在等待财务复核。
    ///
    /// # 返回
    /// 状态为 `PENDING_REVIEW` 时返回 `true`。
    pub fn is_pending_review(&self) -> bool {
        self.status == SettlementStatus::PendingReview
    }

    /// 判断指定账号是否为当前结算经办人。
    ///
    /// # 参数
    /// * `actor_id` - 当前账号标识
    ///
    /// # 返回
    /// 经办人一致时返回 `true`。
    pub fn is_prepared_by(&self, actor_id: &str) -> bool {
        self.prepared_by == actor_id
    }

    /// 校验客户端提交的复核主题与刷新截止策略快照。
    ///
    /// # 参数
    /// * `subject_hash` - 客户端持有的主题摘要
    /// * `cutoff_policy_id` - 客户端持有的刷新截止策略
    /// * `cutoff_policy_version` - 客户端持有的策略版本
    ///
    /// # 返回
    /// 三项均与当前冻结值一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一值不一致时返回领域错误。
    pub fn ensure_review_snapshot(
        &self,
        subject_hash: &str,
        cutoff_policy_id: &str,
        cutoff_policy_version: &str,
    ) -> Result<()> {
        if subject_hash != self.subject_hash
            || cutoff_policy_id != self.refresh_cutoff_policy_id
            || cutoff_policy_version != self.refresh_cutoff_policy_version
        {
            return Err(Error::from("结算主题或刷新截止策略不一致"));
        }
        Ok(())
    }

    /// 计算当前冻结结算事实和差异正式结论的复核主题摘要。
    ///
    /// 摘要不包含可变状态、实体版本或复核结果，因此提交复核和正式决定不会改变
    /// 同一业务主题。
    ///
    /// # 参数
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 返回 64 位小写 SHA-256 十六进制摘要。
    pub fn review_subject_hash(&self, differences: &[SupplierSettlementDifference]) -> String {
        let mut parts = vec![
            "supplier-settlement-review-subject-v1".to_string(),
            self.base.id.clone(),
            self.statement_no.clone(),
            self.supplier_id.to_string(),
            self.period_start.to_string(),
            self.period_end.to_string(),
            self.period_policy_id.clone(),
            self.period_policy_version.clone(),
            self.period_timezone.clone(),
            self.external_bill_no.clone().unwrap_or_default(),
            self.external_bill_version.clone().unwrap_or_default(),
            self.erp_amount.to_string(),
            self.supplier_amount.to_string(),
            self.difference_amount.to_string(),
            self.source_as_of.unix_secs().to_string(),
            self.source_snapshot_at.unix_secs().to_string(),
            self.source_snapshot_hash.clone(),
            self.refresh_cutoff_policy_id.clone(),
            self.refresh_cutoff_policy_version.clone(),
        ];
        let mut differences = differences.iter().collect::<Vec<_>>();
        differences.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        for difference in differences {
            parts.extend([
                difference.base.id.clone(),
                difference.statement_item_id.to_string(),
                difference.difference_type.as_str().to_string(),
                difference.difference_amount.to_string(),
                difference.status.as_str().to_string(),
                difference.resolution.clone().unwrap_or_default(),
                difference.resolved_by.clone().unwrap_or_default(),
                difference
                    .resolved_at
                    .map(|value| value.unix_secs().to_string())
                    .unwrap_or_default(),
            ]);
        }
        digest_parts(&parts)
    }

    /// 校验当前主题摘要与全部差异正式结论一致且没有待处理差异。
    ///
    /// # 参数
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 差异均已处理且主题摘要一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 存在待处理差异或主题摘要过期时返回领域错误。
    pub fn ensure_resolved_subject(&self, differences: &[SupplierSettlementDifference]) -> Result<()> {
        if differences
            .iter()
            .any(|difference| difference.status == SettlementDifferenceStatus::Pending)
        {
            return Err(Error::from("存在未解决差异，禁止提交或确认结算"));
        }
        if self.subject_hash != self.review_subject_hash(differences) {
            return Err(Error::from("结算主题摘要与当前差异结论不一致"));
        }
        Ok(())
    }

    /// 计算 ERP 接受差异对应的冻结成本差额。
    ///
    /// 每个结算明细的 ERP 接受含税差异合计必须精确等于供应商账单与 ERP 冻结
    /// 含税差额；不含税与税额只从冻结三元组派生，禁止按含税金额猜测。
    ///
    /// # 参数
    /// * `items` - 当前结算单的冻结明细
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 返回汇总后的含税、不含税和税额差额。
    ///
    /// # 错误
    /// 明细重复、差异指向其他明细或金额恒等不成立时返回领域错误。
    pub fn accepted_cost_delta(
        &self,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
    ) -> Result<SettlementCostDelta> {
        let statement_id = crate::ids::SupplierSettlementStatementId::new(self.base.id.as_str());
        let mut item_by_id = HashMap::with_capacity(items.len());
        for item in items {
            if !item.belongs_to_statement(&statement_id) {
                return Err(Error::from("结算快照包含其他结算单明细"));
            }
            if item_by_id.insert(item.base.id.as_str(), item).is_some() {
                return Err(Error::from("结算快照包含重复明细"));
            }
        }

        let mut accepted_gross_by_item: HashMap<&str, Amount> = HashMap::new();
        for difference in differences
            .iter()
            .filter(|difference| difference.status == SettlementDifferenceStatus::ErpAcknowledged)
        {
            let item_id = difference.statement_item_id.as_ref();
            if !item_by_id.contains_key(item_id) {
                return Err(Error::from("ERP接受差异未指向当前结算快照明细"));
            }
            accepted_gross_by_item
                .entry(item_id)
                .and_modify(|amount| *amount = amount.checked_add(difference.difference_amount))
                .or_insert(difference.difference_amount);
        }

        let mut total = SettlementCostDelta::zero();
        for (item_id, accepted_gross) in accepted_gross_by_item {
            let delta = item_by_id[item_id].supplier_minus_erp_delta()?;
            if accepted_gross != delta.gross {
                return Err(Error::from(format!(
                    "结算明细 {item_id} 的 ERP 接受差异与冻结双方金额不一致"
                )));
            }
            total.add_assign(delta);
        }
        total.validate()?;
        Ok(total)
    }

    /// 校验结算单具备正式确认条件并返回成本差额。
    ///
    /// # 参数
    /// * `items` - 当前结算单的冻结明细
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 账单身份完整、差异已解决且主题一致时返回冻结成本差额。
    ///
    /// # 错误
    /// 外部账单身份不完整、差异未解决、主题过期或成本差额不一致时返回错误。
    pub fn ensure_confirmable(
        &self,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
    ) -> Result<SettlementCostDelta> {
        if items.is_empty() {
            return Err(Error::from("结算单没有冻结明细"));
        }
        if self.external_bill_no.is_none() || self.external_bill_version.is_none() {
            return Err(Error::from("供应商账单身份未完整冻结"));
        }
        self.ensure_resolved_subject(differences)?;
        self.accepted_cost_delta(items, differences)
    }

    /// 使用新的服务端来源证据批次替换尚未提交复核的草稿快照。
    ///
    /// # 错误
    /// 非草稿态、账单身份/金额/摘要非法时返回错误；供应商、期间与期间策略不可改变。
    pub fn refresh_snapshot(&mut self, update: SupplierSettlementSnapshotUpdate) -> Result<()> {
        if !matches!(
            self.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        ) {
            return Err(Error::from("当前结算状态禁止刷新来源快照"));
        }
        let external_bill_no = normalize_required_text(
            update.external_bill_no,
            "外部账单号不能为空",
            EXTERNAL_BILL_NO_MAX_LEN,
            "外部账单号过长",
        )?;
        let external_bill_version = normalize_required_text(
            update.external_bill_version,
            "外部账单版本不能为空",
            EXTERNAL_BILL_NO_MAX_LEN,
            "外部账单版本过长",
        )?;
        ensure_amount_non_negative(update.erp_amount, "ERP 结算金额不得为负")?;
        ensure_amount_non_negative(update.supplier_amount, "供应商结算金额不得为负")?;
        self.external_bill_no = Some(external_bill_no);
        self.external_bill_version = Some(external_bill_version);
        self.erp_amount = update.erp_amount;
        self.supplier_amount = update.supplier_amount;
        self.difference_amount = update.supplier_amount.checked_sub(update.erp_amount);
        self.source_as_of = update.source_as_of;
        self.source_snapshot_at = update.source_snapshot_at;
        self.source_snapshot_hash = normalize_sha256(update.source_snapshot_hash, "来源快照摘要")?;
        self.status = if update.has_difference {
            SettlementStatus::HasDifference
        } else {
            SettlementStatus::PendingReconciliation
        };
        Ok(())
    }

    /// 更新结算单。
    ///
    /// 复用 `new` 的校验规则并强制状态守卫（已作废终态、已确认只能作废）；
    /// 金额变更按恒等式重算 `difference_amount`；推进到已确认时必须提供应付账户，
    /// 缺失时补记确认时间。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态迁移非法、金额为负、复核人为空/超长或与经办人相同、确认缺少应付账户时返回错误。
    pub fn update(&mut self, update: SupplierSettlementStatementUpdate) -> Result<()> {
        let target_status = update.status.unwrap_or(self.status);
        if target_status != self.status
            && matches!(
                target_status,
                SettlementStatus::PendingReview | SettlementStatus::Confirmed
            )
        {
            return Err(Error::from("提交复核与确认必须使用供应商结算强类型领域命令"));
        }
        ensure_status_move(self.status, target_status)?;
        if let Some(erp_amount) = update.erp_amount {
            ensure_amount_non_negative(erp_amount, "ERP 结算金额不得为负")?;
            self.erp_amount = erp_amount;
        }
        if let Some(supplier_amount) = update.supplier_amount {
            ensure_amount_non_negative(supplier_amount, "供应商结算金额不得为负")?;
            self.supplier_amount = supplier_amount;
        }
        self.difference_amount = self.supplier_amount.checked_sub(self.erp_amount);
        if let Some(reviewed_by) = update.reviewed_by {
            self.apply_reviewed_by(reviewed_by)?;
        }
        if let Some(status) = update.status {
            self.apply_status(status, update.payable_account_id)?;
        }
        Ok(())
    }

    /// 更新差异结论变化后的主题摘要。
    ///
    /// 来源快照保持冻结；只有覆盖当前结算明细与差异正式结论的主题摘要允许推进。
    /// 待复核、已确认和已作废状态均拒绝修改。
    ///
    /// # 错误
    /// 当前状态不可编辑或摘要不是规范 SHA-256 十六进制值时返回错误。
    pub fn update_subject_hash(&mut self, subject_hash: impl Into<String>) -> Result<()> {
        if matches!(
            self.status,
            SettlementStatus::PendingReview | SettlementStatus::Confirmed | SettlementStatus::Voided
        ) {
            return Err(Error::from("当前结算状态禁止改变复核主题"));
        }
        self.subject_hash = normalize_sha256(subject_hash.into(), "主题摘要")?;
        Ok(())
    }

    /// 标记当前可编辑结算草稿仍存在正式差异。
    ///
    /// # 返回
    /// 状态推进到 `HAS_DIFFERENCE` 时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前结算单已提交复核或进入终态时返回领域错误。
    pub fn mark_has_difference(&mut self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("当前结算状态禁止登记差异结论"));
        }
        self.status = SettlementStatus::HasDifference;
        Ok(())
    }

    /// 作废尚未提交复核的可编辑结算草稿。
    ///
    /// # 返回
    /// 状态推进到 `VOIDED` 时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前结算单已提交复核或进入终态时返回领域错误。
    pub fn void_draft(&mut self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("当前结算状态禁止作废草稿"));
        }
        self.status = SettlementStatus::Voided;
        Ok(())
    }

    /// 将当前冻结主题提交财务复核。
    ///
    /// 本方法只形成结算单状态事实；应用服务必须把唯一复核任务与审计写入同一事务。
    ///
    /// # 错误
    /// 非草稿、待对账或有差异状态时返回错误。
    pub fn submit_review(&mut self) -> Result<()> {
        if !matches!(
            self.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        ) {
            return Err(Error::from("当前结算状态不允许提交复核"));
        }
        self.status = SettlementStatus::PendingReview;
        Ok(())
    }

    /// 记录强类型结算复核决定。
    ///
    /// 确认形成应付并进入终态；驳回必须携带结构化原因，且只能回到草稿或有差异。
    /// 经办人与复核人岗位分离由本实体再次固化。
    ///
    /// # 错误
    /// 非待复核状态、岗位冲突、驳回原因非法或目标状态非法时返回错误。
    pub fn record_review(
        &mut self,
        decision: SettlementReviewDecision,
        reviewed_by: impl Into<String>,
        reviewed_at: Instant,
    ) -> Result<()> {
        if self.status != SettlementStatus::PendingReview {
            return Err(Error::from("仅待复核结算单可以记录正式决定"));
        }
        let reviewed_by =
            normalize_required_text(reviewed_by.into(), "复核人不能为空", ACTOR_MAX_LEN, "复核人过长")?;
        if reviewed_by == self.prepared_by {
            return Err(Error::from("经办人与复核人不得相同"));
        }
        let (result, target_status, reason_code, comment, payable_account_id) = match decision {
            SettlementReviewDecision::Confirm {
                payable_account_id,
                comment,
            } => (
                SettlementReviewResult::Confirmed,
                SettlementStatus::Confirmed,
                None,
                normalize_optional_text(comment, "复核说明", REVIEW_COMMENT_MAX_LEN)?,
                Some(payable_account_id),
            ),
            SettlementReviewDecision::Reject {
                return_status,
                reason_code,
                comment,
            } => {
                if !matches!(
                    return_status,
                    SettlementStatus::Draft | SettlementStatus::HasDifference
                ) {
                    return Err(Error::from("驳回复核只能退回草稿或有差异状态"));
                }
                (
                    SettlementReviewResult::Rejected,
                    return_status,
                    Some(reason_code.as_str().to_string()),
                    normalize_optional_text(comment, "复核说明", REVIEW_COMMENT_MAX_LEN)?,
                    None,
                )
            }
        };
        self.status = target_status;
        self.reviewed_by = Some(reviewed_by);
        self.review_result = Some(result);
        self.review_reason_code = reason_code;
        self.review_comment = comment;
        self.reviewed_at = Some(reviewed_at);
        self.payable_account_id = payable_account_id;
        self.confirmed_at = (result == SettlementReviewResult::Confirmed).then_some(reviewed_at);
        Ok(())
    }

    /// 应用复核人更新。
    ///
    /// # 参数
    /// * `reviewed_by` - 新的复核人
    ///
    /// # 错误
    /// 复核人为空/超长或与经办人相同时返回错误。
    fn apply_reviewed_by(&mut self, reviewed_by: String) -> Result<()> {
        let reviewed_by =
            normalize_required_text(reviewed_by, "复核人不能为空", ACTOR_MAX_LEN, "复核人过长")?;
        if reviewed_by == self.prepared_by {
            return Err(Error::from("经办人与复核人不得相同"));
        }
        self.reviewed_by = Some(reviewed_by);
        Ok(())
    }

    /// 应用状态更新并维护确认字段。
    ///
    /// # 参数
    /// * `status` - 新的状态
    /// * `payable_account_id` - 应付账户（推进到已确认时必填）
    ///
    /// # 错误
    /// 推进到已确认但缺少应付账户时返回错误。
    fn apply_status(
        &mut self,
        status: SettlementStatus,
        payable_account_id: Option<PayableAccountId>,
    ) -> Result<()> {
        if status == SettlementStatus::Confirmed {
            let payable_account_id =
                payable_account_id.ok_or_else(|| Error::from("确认时必须提供应付账户"))?;
            self.confirmed_at.get_or_insert_with(Instant::now);
            self.payable_account_id = Some(payable_account_id);
        }
        self.status = status;
        Ok(())
    }
}

/// 对字段逐项加入长度前缀后计算稳定摘要，消除字符串拼接歧义。
fn digest_parts(parts: &[String]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 校验金额非负。
///
/// # 参数
/// * `value` - 金额
/// * `message` - 失败时的错误信息
///
/// # 错误
/// 金额为负时返回错误。
fn ensure_amount_non_negative(value: Amount, message: &str) -> Result<()> {
    if value.to_decimal() < Decimal::ZERO {
        return Err(Error::from(message));
    }
    Ok(())
}

/// 规范化服务端生成的 SHA-256 十六进制摘要。
fn normalize_sha256(value: String, field: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != HASH_LEN || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::from(format!("{field}必须是64位SHA-256十六进制摘要")));
    }
    Ok(value)
}

/// 校验结算状态迁移守卫（保守守卫：已作废终态，已确认只能作废；幂等恒合法）。
///
/// # 参数
/// * `from` - 迁移前状态
/// * `to` - 目标状态
///
/// # 错误
/// 已作废再变更、或已确认迁移到已确认以外的状态时返回错误。
fn ensure_status_move(from: SettlementStatus, to: SettlementStatus) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if from == SettlementStatus::Voided {
        return Err(Error::from("已作废结算单不可再变更状态"));
    }
    if from == SettlementStatus::Confirmed && to != SettlementStatus::Voided {
        return Err(Error::from("已确认结算单只能作废"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::time::BusinessDate;
    use crate::ids::{
        PayableAccountId, SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
    };
    use crate::money::Quantity;
    use crate::supplier_settlement::{
        SettlementDifferenceConclusion, SettlementDifferenceConclusionKind, SettlementDifferenceType,
        SupplierSettlementDifferenceData, SupplierSettlementItemData,
    };
    use std::str::FromStr;

    fn sample_data() -> SupplierSettlementStatementData {
        SupplierSettlementStatementData {
            statement_no: " ST-2026-001 ".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "calendar-month".to_string(),
            period_policy_version: "1".to_string(),
            period_timezone: "Asia/Shanghai".to_string(),
            external_bill_no: None,
            external_bill_version: None,
            erp_amount: Amount::from_str("1000.00").unwrap(),
            supplier_amount: Amount::from_str("1023.45").unwrap(),
            subject_hash: "a".repeat(64),
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
            source_snapshot_hash: "b".repeat(64),
            refresh_cutoff_policy_id: "supplier-settlement-review-cutoff".to_string(),
            refresh_cutoff_policy_version: "1".to_string(),
            prepared_by: " 经办人-a ".to_string(),
        }
    }

    #[test]
    fn new_accepts_draft_and_computes_difference() {
        let statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            sample_data(),
        )
        .unwrap();

        assert_eq!(statement.statement_no, "ST-2026-001");
        assert_eq!(statement.prepared_by, "经办人-a");
        assert_eq!(statement.difference_amount, Amount::from_str("23.45").unwrap());
        assert_eq!(statement.status, SettlementStatus::Draft);
    }

    #[test]
    fn new_rejects_reversed_period() {
        let data = SupplierSettlementStatementData {
            period_start: BusinessDate::from_ymd(2026, 8, 1).unwrap(),
            ..sample_data()
        };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-2"), data)
                .is_err()
        );
    }

    #[test]
    fn new_rejects_partial_external_bill_identity() {
        let data = SupplierSettlementStatementData {
            external_bill_no: Some("BILL-1".to_string()),
            external_bill_version: None,
            ..sample_data()
        };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-4"), data)
                .is_err()
        );
    }

    #[test]
    fn new_rejects_negative_amounts() {
        let data = SupplierSettlementStatementData {
            erp_amount: Amount::from_str("-1.00").unwrap(),
            ..sample_data()
        };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-5"), data)
                .is_err()
        );
    }

    #[test]
    fn new_always_starts_as_unreviewed_draft() {
        let statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-6"),
            sample_data(),
        )
        .unwrap();

        assert_eq!(statement.status, SettlementStatus::Draft);
        assert_eq!(statement.reviewed_by, None);
        assert_eq!(statement.review_result, None);
        assert_eq!(statement.confirmed_at, None);
        assert_eq!(statement.payable_account_id, None);
    }

    #[test]
    fn new_rejects_empty_statement_no() {
        let data = SupplierSettlementStatementData {
            statement_no: "   ".to_string(),
            ..sample_data()
        };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-8"), data)
                .is_err()
        );
    }

    #[test]
    fn record_review_confirms_with_payable_and_recomputes_difference() {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            sample_data(),
        )
        .unwrap();
        statement
            .update(SupplierSettlementStatementUpdate {
                erp_amount: Some(Amount::from_str("1010.00").unwrap()),
                supplier_amount: Some(Amount::from_str("1030.00").unwrap()),
                ..Default::default()
            })
            .unwrap();
        statement.submit_review().unwrap();
        statement
            .record_review(
                SettlementReviewDecision::Confirm {
                    payable_account_id: PayableAccountId::new("payable-account-1"),
                    comment: Some("核对无误".to_string()),
                },
                "复核人-b",
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();

        assert_eq!(statement.status, SettlementStatus::Confirmed);
        assert_eq!(statement.difference_amount, Amount::from_str("20.00").unwrap());
        assert_eq!(statement.reviewed_by.as_deref(), Some("复核人-b"));
        assert_eq!(
            statement.confirmed_at,
            Some(Instant::from_unix_secs(1_700_000_100))
        );
        assert_eq!(
            statement.payable_account_id,
            Some(PayableAccountId::new("payable-account-1"))
        );
        assert_eq!(statement.statement_no, "ST-2026-001", "关键字段不可修改");
    }

    #[test]
    fn strong_review_rejects_bypass_and_status_regression() {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            sample_data(),
        )
        .unwrap();
        assert!(
            statement
                .update(SupplierSettlementStatementUpdate {
                    status: Some(SettlementStatus::Confirmed),
                    ..Default::default()
                })
                .is_err(),
            "确认必须经过强类型复核命令"
        );

        statement.submit_review().unwrap();
        statement
            .record_review(
                SettlementReviewDecision::Confirm {
                    payable_account_id: PayableAccountId::new("payable-account-1"),
                    comment: None,
                },
                "复核人-b",
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();
        assert!(
            statement
                .update(SupplierSettlementStatementUpdate {
                    status: Some(SettlementStatus::Draft),
                    ..Default::default()
                })
                .is_err(),
            "已确认结算单只能作废"
        );

        statement
            .update(SupplierSettlementStatementUpdate {
                status: Some(SettlementStatus::Voided),
                ..Default::default()
            })
            .unwrap();
        assert!(
            statement
                .update(SupplierSettlementStatementUpdate {
                    status: Some(SettlementStatus::Draft),
                    ..Default::default()
                })
                .is_err(),
            "已作废结算单不可再变更状态"
        );
    }

    #[test]
    fn rejected_review_requires_reason_and_returns_to_editable_state() {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            sample_data(),
        )
        .unwrap();
        statement.submit_review().unwrap();

        assert!(SettlementReviewRejectReason::parse("   ").is_err());
        assert!(SettlementReviewRejectReason::parse("AMOUNT_UNRESOLVED").is_err());
        statement
            .record_review(
                SettlementReviewDecision::Reject {
                    return_status: SettlementStatus::HasDifference,
                    reason_code: SettlementReviewRejectReason::AmountMismatch,
                    comment: Some("差异证据不足".to_string()),
                },
                "复核人-b",
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();

        assert_eq!(statement.status, SettlementStatus::HasDifference);
        assert_eq!(statement.review_result, Some(SettlementReviewResult::Rejected));
        assert_eq!(statement.review_reason_code.as_deref(), Some("AMOUNT_MISMATCH"));
    }

    #[test]
    fn confirmable_conditions_and_subject_hash_are_owned_by_statement() {
        let mut data = sample_data();
        data.external_bill_no = Some("BILL-1".to_string());
        data.external_bill_version = Some("1".to_string());
        data.erp_amount = Amount::from_str("100.00").unwrap();
        data.supplier_amount = Amount::from_str("101.00").unwrap();
        let mut statement =
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-1"), data)
                .unwrap();
        let item = SupplierSettlementItem::new(
            SupplierSettlementItemId::new("item-1"),
            SupplierSettlementItemData {
                statement_id: SupplierSettlementStatementId::new("statement-1"),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("fulfillment-item-1"),
                quantity: Quantity::from_str("1").unwrap(),
                order_amount: Amount::from_str("100.00").unwrap(),
                freight_amount: Amount::from_str("0.00").unwrap(),
                service_fee_amount: Amount::from_str("0.00").unwrap(),
                refund_amount: Amount::from_str("0.00").unwrap(),
                erp_calculated_amount: Amount::from_str("100.00").unwrap(),
                erp_calculated_net_amount: Amount::from_str("87.00").unwrap(),
                erp_calculated_tax_amount: Amount::from_str("13.00").unwrap(),
                supplier_billed_amount: Amount::from_str("101.00").unwrap(),
                supplier_billed_net_amount: Amount::from_str("87.87").unwrap(),
                supplier_billed_tax_amount: Amount::from_str("13.13").unwrap(),
            },
        )
        .unwrap();
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            SupplierSettlementDifferenceData {
                statement_item_id: SupplierSettlementItemId::new("item-1"),
                difference_type: SettlementDifferenceType::Amount,
                difference_amount: Amount::from_str("1.00").unwrap(),
                status: SettlementDifferenceStatus::Pending,
                resolution: None,
                resolved_by: None,
                resolved_at: None,
            },
        )
        .unwrap();
        let conclusion = SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::ErpAccepted,
            "ACCEPT_BILL",
            vec!["proof-1".to_string()],
        )
        .unwrap();
        difference
            .record_conclusion(&conclusion, "finance-1", Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        let differences = vec![difference];
        statement
            .update_subject_hash(statement.review_subject_hash(&differences))
            .unwrap();

        let delta = statement
            .ensure_confirmable(std::slice::from_ref(&item), &differences)
            .unwrap();
        assert_eq!(delta.gross, Amount::from_str("1.00").unwrap());
        assert_eq!(delta.net, Amount::from_str("0.87").unwrap());
        assert_eq!(delta.tax, Amount::from_str("0.13").unwrap());
        assert!(statement.ensure_version(statement.base.version).is_ok());
        assert!(statement.ensure_version(statement.base.version + 1).is_err());

        let mut pending = differences[0].clone();
        pending.status = SettlementDifferenceStatus::Pending;
        assert!(statement.ensure_confirmable(&[item], &[pending]).is_err());
    }
}
