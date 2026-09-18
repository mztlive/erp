//! `supplier_settlement_statement`（数据模型 §6.20 供应商周期结算单）。
//!
//! 结算单是正式单据：`statement_no`、供应商、结算期间与外部账单身份创建后不可修改；
//! 经办人与复核人不得相同；`difference_amount` 由双方金额派生并强制恒等；已确认状态
//! 必须携带确认时间与应付账户，已作废为终态。

use std::collections::HashMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{PayableAccountId, SupplierAccountId, SupplierSettlementStatementId};
use erp_core::money::Amount;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::difference::{SettlementDifferenceStatus, SupplierSettlementDifference};
use super::item::{SettlementCostDelta, SupplierSettlementItem};
use super::review_reason::SettlementReviewRejectReason;

/// 结算单号最大长度。
const STATEMENT_NO_MAX_LEN: usize = 64;
/// 外部账单号/版本最大长度。
const EXTERNAL_BILL_NO_MAX_LEN: usize = 64;
/// 对账负责人/差异处理人/复核人标识最大长度。
pub(crate) const ACTOR_MAX_LEN: usize = 128;
/// SHA-256 十六进制摘要长度。
const HASH_LEN: usize = 64;
/// 冻结策略标识/版本最大长度。
const POLICY_VALUE_MAX_LEN: usize = 128;
/// 复核说明最大长度。
const REVIEW_COMMENT_MAX_LEN: usize = 512;

mod hash;
mod review;
mod status;

pub use review::{SettlementReviewDecision, SettlementReviewResult};
pub use status::SettlementStatus;

use self::hash::normalize_sha256;

/// 结算主题与来源快照共用的长度前缀摘要（消除字符串拼接歧义）。
///
/// # 参数
/// * `parts` - 按业务语义排序后的字段集合
///
/// # 返回
/// 返回 64 位小写 SHA-256 十六进制摘要。
pub fn statement_digest_parts(parts: &[String]) -> String {
    self::hash::digest_parts(parts)
}

/// 规范化服务端生成的 SHA-256 十六进制摘要。
///
/// # 参数
/// * `value` - 待规范化的摘要文本
/// * `field` - 错误消息使用的业务字段名称
///
/// # 返回
/// 返回小写 64 位十六进制摘要。
///
/// # 错误
/// 非 64 位十六进制时返回领域错误。
pub fn normalize_statement_sha256(value: String, field: &str) -> Result<String> {
    normalize_sha256(value, field)
}
use self::status::ensure_status_move;

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
    /// 对账负责人。
    pub prepared_by: String,
    /// 业务组织（内部组织 ID）。
    pub business_org_unit_id: String,
    /// 差异处理人；空则等于对账负责人。
    pub difference_handler_user_id: String,
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
    /// 对账负责人。
    pub prepared_by: String,
    /// 业务组织（内部组织 ID）；不得写入结算主体或 `"company"` 根。
    #[serde(default)]
    pub business_org_unit_id: String,
    /// 差异处理人；缺省等于对账负责人，可独立改派。
    #[serde(default)]
    pub difference_handler_user_id: String,
    /// 实际复核人。
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
    /// * `id` - 实体主键（`erp_core::ids::SupplierSettlementStatementId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的结算单实体。
    ///
    /// # 错误
    /// 单号为空/超长、期间倒挂、账单身份不完整、经办复核相同、金额为负或
    /// 确认状态字段不一致时返回错误。
    pub fn new(id: SupplierSettlementStatementId, data: SupplierSettlementStatementData) -> Result<Self> {
        let (prepared_by, business_org_unit_id, difference_handler_user_id) = normalize_ownership(&data)?;
        let statement_no = normalize_required_text(
            data.statement_no,
            "结算单号不能为空",
            STATEMENT_NO_MAX_LEN,
            "结算单号过长",
        )?;
        let external_bill_no =
            normalize_optional_text(data.external_bill_no, "外部账单号", EXTERNAL_BILL_NO_MAX_LEN)?;
        let external_bill_version =
            normalize_optional_text(data.external_bill_version, "外部账单版本", EXTERNAL_BILL_NO_MAX_LEN)?;
        if external_bill_no.is_some() != external_bill_version.is_some() {
            return Err(Error::from("外部账单号与版本必须同时提供或同时省略"));
        }
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
            business_org_unit_id,
            difference_handler_user_id,
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
    /// 判断指定账号是否为当前对账负责人。
    ///
    /// # 参数
    /// * `actor_id` - 当前账号标识
    ///
    /// # 返回
    /// 对账负责人一致时返回 `true`。
    pub fn is_prepared_by(&self, actor_id: &str) -> bool {
        self.prepared_by == actor_id
    }

    /// 返回当前差异处理人；存量空值回退对账负责人。
    ///
    /// # 返回
    /// 返回差异处理人 ID。
    pub fn difference_handler(&self) -> &str {
        if self.difference_handler_user_id.is_empty() {
            &self.prepared_by
        } else {
            &self.difference_handler_user_id
        }
    }

    /// 判断指定账号是否为当前差异处理人。
    ///
    /// # 参数
    /// * `actor_id` - 当前账号标识
    ///
    /// # 返回
    /// 差异处理人一致时返回 `true`。
    pub fn is_difference_handler(&self, actor_id: &str) -> bool {
        self.difference_handler() == actor_id
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
            && matches!(target_status, SettlementStatus::PendingReview | SettlementStatus::Confirmed)
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
}

fn normalize_ownership(data: &SupplierSettlementStatementData) -> Result<(String, String, String)> {
    let prepared_by = normalize_required_text(
        data.prepared_by.clone(),
        "对账负责人不能为空",
        ACTOR_MAX_LEN,
        "对账负责人过长",
    )?;
    let business_org_unit_id = normalize_required_text(
        data.business_org_unit_id.clone(),
        "业务组织不能为空",
        ACTOR_MAX_LEN,
        "业务组织过长",
    )?;
    if business_org_unit_id == "company" {
        return Err(Error::from("业务组织不能使用公司根"));
    }
    let difference_handler_user_id = if data.difference_handler_user_id.trim().is_empty() {
        prepared_by.clone()
    } else {
        normalize_required_text(
            data.difference_handler_user_id.clone(),
            "差异处理人不能为空",
            ACTOR_MAX_LEN,
            "差异处理人过长",
        )?
    };
    Ok((prepared_by, business_org_unit_id, difference_handler_user_id))
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{
        PayableAccountId, SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
    };
    use erp_core::money::Quantity;

    use super::*;
    use crate::entity::supplier_settlement::{
        SettlementDifferenceConclusion, SettlementDifferenceConclusionKind, SettlementDifferenceType,
        SupplierSettlementDifferenceData, SupplierSettlementItemData,
    };

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
            business_org_unit_id: " org-finance ".to_string(),
            difference_handler_user_id: String::new(),
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
        assert_eq!(statement.business_org_unit_id, "org-finance");
        assert_eq!(statement.difference_handler(), "经办人-a");
        assert_eq!(statement.difference_amount, Amount::from_str("23.45").unwrap());
        assert_eq!(statement.status, SettlementStatus::Draft);
    }

    #[test]
    fn new_rejects_company_root_as_business_org() {
        let data =
            SupplierSettlementStatementData { business_org_unit_id: "company".into(), ..sample_data() };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-company"), data)
                .is_err()
        );
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
        let data = SupplierSettlementStatementData { statement_no: "   ".to_string(), ..sample_data() };
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
        assert_eq!(statement.confirmed_at, Some(Instant::from_unix_secs(1_700_000_100)));
        assert_eq!(statement.payable_account_id, Some(PayableAccountId::new("payable-account-1")));
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
        statement.update_subject_hash(statement.review_subject_hash(&differences)).unwrap();

        let delta = statement.ensure_confirmable(std::slice::from_ref(&item), &differences).unwrap();
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
