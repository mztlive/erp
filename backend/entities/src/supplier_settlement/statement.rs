//! `supplier_settlement_statement`（数据模型 §6.20 供应商周期结算单）。
//!
//! 结算单是正式单据：`statement_no`、供应商、结算期间与外部账单身份创建后不可修改；
//! 经办人与复核人不得相同；`difference_amount` 由双方金额派生并强制恒等；已确认状态
//! 必须携带确认时间与应付账户，已作废为终态。

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{PayableAccountId, SupplierAccountId, SupplierSettlementStatementId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

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
/// 复核原因代码最大长度。
const REVIEW_REASON_CODE_MAX_LEN: usize = 64;
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
        /// 结构化驳回原因代码。
        reason_code: String,
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
                    Some(normalize_required_text(
                        reason_code,
                        "驳回原因代码不能为空",
                        REVIEW_REASON_CODE_MAX_LEN,
                        "驳回原因代码过长",
                    )?),
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
    use crate::ids::{PayableAccountId, SupplierAccountId, SupplierSettlementStatementId};
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

        assert!(statement
            .record_review(
                SettlementReviewDecision::Reject {
                    return_status: SettlementStatus::Draft,
                    reason_code: "   ".to_string(),
                    comment: None,
                },
                "复核人-b",
                Instant::from_unix_secs(1_700_000_100),
            )
            .is_err());
        statement
            .record_review(
                SettlementReviewDecision::Reject {
                    return_status: SettlementStatus::HasDifference,
                    reason_code: "AMOUNT_UNRESOLVED".to_string(),
                    comment: Some("差异证据不足".to_string()),
                },
                "复核人-b",
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();

        assert_eq!(statement.status, SettlementStatus::HasDifference);
        assert_eq!(statement.review_result, Some(SettlementReviewResult::Rejected));
        assert_eq!(statement.review_reason_code.as_deref(), Some("AMOUNT_UNRESOLVED"));
    }
}
