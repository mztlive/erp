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
    /// 供应商账单号，可空（与版本成对出现）。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本，可空。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: Amount,
    /// 供应商金额。
    pub supplier_amount: Amount,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 经办人。
    pub prepared_by: String,
    /// 复核人，可空。
    pub reviewed_by: Option<String>,
    /// 确认时间（已确认状态必填）。
    pub confirmed_at: Option<Instant>,
    /// 确认后形成的应付账户（已确认状态必填）。
    pub payable_account_id: Option<PayableAccountId>,
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
    /// 经办人。
    pub prepared_by: String,
    /// 复核人。
    pub reviewed_by: Option<String>,
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
        let reviewed_by = normalize_optional_text(data.reviewed_by, "复核人", ACTOR_MAX_LEN)?;
        if let Some(reviewed_by) = &reviewed_by {
            if *reviewed_by == prepared_by {
                return Err(Error::from("经办人与复核人不得相同"));
            }
        }
        if data.period_end < data.period_start {
            return Err(Error::from("结算期间结束不得早于开始"));
        }
        ensure_amount_non_negative(data.erp_amount, "ERP 结算金额不得为负")?;
        ensure_amount_non_negative(data.supplier_amount, "供应商结算金额不得为负")?;
        ensure_confirmation_state(data.status, data.confirmed_at, &data.payable_account_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            statement_no,
            supplier_id: data.supplier_id,
            period_start: data.period_start,
            period_end: data.period_end,
            external_bill_no,
            external_bill_version,
            erp_amount: data.erp_amount,
            supplier_amount: data.supplier_amount,
            difference_amount: data.supplier_amount.checked_sub(data.erp_amount),
            status: data.status,
            prepared_by,
            reviewed_by,
            confirmed_at: data.confirmed_at,
            payable_account_id: data.payable_account_id,
        })
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

/// 校验确认状态与确认字段的一致性（§6.20）。
///
/// # 参数
/// * `status` - 结算状态
/// * `confirmed_at` - 确认时间
/// * `payable_account_id` - 应付账户
///
/// # 错误
/// 确认时间与应付账户不成对、或已确认状态缺少两者时返回错误。
fn ensure_confirmation_state(
    status: SettlementStatus,
    confirmed_at: Option<Instant>,
    payable_account_id: &Option<PayableAccountId>,
) -> Result<()> {
    if confirmed_at.is_some() != payable_account_id.is_some() {
        return Err(Error::from("确认时间与应付账户必须同时提供或同时省略"));
    }
    if status == SettlementStatus::Confirmed && confirmed_at.is_none() {
        return Err(Error::from("已确认状态必须提供确认时间和应付账户"));
    }
    Ok(())
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
            external_bill_no: None,
            external_bill_version: None,
            erp_amount: Amount::from_str("1000.00").unwrap(),
            supplier_amount: Amount::from_str("1023.45").unwrap(),
            status: SettlementStatus::Draft,
            prepared_by: " 经办人-a ".to_string(),
            reviewed_by: None,
            confirmed_at: None,
            payable_account_id: None,
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
    fn new_rejects_same_preparer_and_reviewer() {
        let data = SupplierSettlementStatementData {
            reviewed_by: Some("经办人-a".to_string()),
            ..sample_data()
        };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-3"), data)
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
    fn new_rejects_confirmed_without_payable_account() {
        let data = SupplierSettlementStatementData {
            status: SettlementStatus::Confirmed,
            ..sample_data()
        };
        assert!(
            SupplierSettlementStatement::new(SupplierSettlementStatementId::new("statement-6"), data)
                .is_err()
        );

        let half_pair = SupplierSettlementStatementData {
            status: SettlementStatus::Confirmed,
            confirmed_at: Some(Instant::from_unix_secs(1_700_000_000)),
            payable_account_id: None,
            ..sample_data()
        };
        assert!(SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-7"),
            half_pair
        )
        .is_err());
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
    fn update_confirms_with_payable_and_recomputes_difference() {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            sample_data(),
        )
        .unwrap();
        statement
            .update(SupplierSettlementStatementUpdate {
                erp_amount: Some(Amount::from_str("1010.00").unwrap()),
                supplier_amount: Some(Amount::from_str("1030.00").unwrap()),
                status: Some(SettlementStatus::Confirmed),
                reviewed_by: Some("复核人-b".to_string()),
                payable_account_id: Some(PayableAccountId::new("payable-account-1")),
            })
            .unwrap();

        assert_eq!(statement.status, SettlementStatus::Confirmed);
        assert_eq!(statement.difference_amount, Amount::from_str("20.00").unwrap());
        assert_eq!(statement.reviewed_by.as_deref(), Some("复核人-b"));
        assert!(statement.confirmed_at.is_some());
        assert_eq!(
            statement.payable_account_id,
            Some(PayableAccountId::new("payable-account-1"))
        );
        assert_eq!(statement.statement_no, "ST-2026-001", "关键字段不可修改");
    }

    #[test]
    fn update_rejects_status_regression_and_conflicts() {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            sample_data(),
        )
        .unwrap();
        assert!(
            statement
                .update(SupplierSettlementStatementUpdate {
                    status: Some(SettlementStatus::Confirmed),
                    payable_account_id: None,
                    ..Default::default()
                })
                .is_err(),
            "确认必须提供应付账户"
        );

        statement
            .update(SupplierSettlementStatementUpdate {
                status: Some(SettlementStatus::Confirmed),
                payable_account_id: Some(PayableAccountId::new("payable-account-1")),
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
}
