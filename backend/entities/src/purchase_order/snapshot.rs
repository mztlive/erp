//! 采购单据的结构化快照（数据模型 §4.4：正式版本内联结构化快照，非 JSON blob）。
//!
//! 快照字段由 P3 在形成提交/版本时填充，P1 只负责定义与校验。

use chrono::{Datelike, Days};
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::money::{Amount, Rate};
use crate::supplier::business_category::split_encoded_payment_term_snapshot;
use crate::validation::normalize_required_text;

/// 供应商名称最大长度。
const SUPPLIER_NAME_MAX_LEN: usize = 256;
/// 付款条件代码最大长度。
const PAYMENT_TERM_MAX_LEN: usize = 64;

/// 提交/生效时点的供应商快照（§6.6 `supplier_snapshot`）。
///
/// 只保存结构化业务字段（当前为供应商名称），基础资料后续修改不改变历史单据（§4.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierSnapshot {
    /// 供应商名称。
    pub supplier_name: String,
}

impl SupplierSnapshot {
    /// 创建供应商快照。
    ///
    /// 校验并规范化供应商名称（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `supplier_name` - 供应商名称
    ///
    /// # 返回
    /// 返回快照实例。
    ///
    /// # 错误
    /// 名称为空或超长时返回错误。
    pub fn new(supplier_name: String) -> Result<Self> {
        let supplier_name = normalize_required_text(
            supplier_name,
            "供应商名称不能为空",
            SUPPLIER_NAME_MAX_LEN,
            "供应商名称过长",
        )?;
        Ok(Self { supplier_name })
    }
}

/// 付款条件与先款后货门禁快照（§6.6 `payment_term_snapshot`）。
///
/// 保存付款条件代码、是否先款后货（PREPAY）以及履约前最低有效付款金额/比例；
/// 金额/比例为可选的冻结门槛，由 P3 在提交时按供应商商业资料快照填充。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentTermSnapshot {
    /// 付款条件代码。
    pub payment_term_code: String,
    /// 是否先款后货（PREPAY）。
    pub prepay_gate: bool,
    /// 履约前最低有效付款金额。
    pub prepay_minimum_amount: Option<Amount>,
    /// 履约前最低有效付款比例。
    pub prepay_minimum_ratio: Option<Rate>,
}

impl PaymentTermSnapshot {
    /// 创建付款条件快照。
    ///
    /// 校验并规范化付款条件代码，并拒绝负的金额/比例门槛。
    ///
    /// # 参数
    /// * `payment_term_code` - 付款条件代码
    /// * `prepay_gate` - 是否先款后货
    /// * `prepay_minimum_amount` - 履约前最低有效付款金额（可空）
    /// * `prepay_minimum_ratio` - 履约前最低有效付款比例（可空）
    ///
    /// # 返回
    /// 返回快照实例。
    ///
    /// # 错误
    /// 代码为空/超长，或金额/比例门槛为负时返回错误。
    pub fn new(
        payment_term_code: String,
        prepay_gate: bool,
        prepay_minimum_amount: Option<Amount>,
        prepay_minimum_ratio: Option<Rate>,
    ) -> Result<Self> {
        let payment_term_code = normalize_required_text(
            payment_term_code,
            "付款条件不能为空",
            PAYMENT_TERM_MAX_LEN,
            "付款条件过长",
        )?;
        if let Some(amount) = prepay_minimum_amount {
            if amount.to_decimal() < rust_decimal::Decimal::ZERO {
                return Err(Error::from("先款门槛金额不能为负"));
            }
        }
        if let Some(ratio) = prepay_minimum_ratio {
            if ratio.to_decimal() < rust_decimal::Decimal::ZERO {
                return Err(Error::from("先款门槛比例不能为负"));
            }
        }
        Ok(Self {
            payment_term_code,
            prepay_gate,
            prepay_minimum_amount,
            prepay_minimum_ratio,
        })
    }

    /// 计算采购应付的计划付款日。
    ///
    /// 先款/现结条件以采购最终通过日为付款日；货到 15/30 天条件以提交行中
    /// 最晚预计交付日为基准。未知付款条件不得静默回退为当天。
    ///
    /// # 参数
    /// * `approved_on` - 采购单最终通过的业务日期
    /// * `expected_delivery_on` - 商品/服务行最晚预计交付日
    ///
    /// # 返回
    /// 返回由冻结付款条件确定的计划付款日。
    ///
    /// # 错误
    /// 后付条件缺少预计交付日、条件未登记规则或日期溢出时返回错误。
    pub fn payable_due_date(
        &self,
        approved_on: BusinessDate,
        expected_delivery_on: Option<BusinessDate>,
    ) -> Result<BusinessDate> {
        let code = split_encoded_payment_term_snapshot(&self.payment_term_code)
            .payment_term_code
            .trim()
            .to_uppercase();
        if self.prepay_gate
            || code.starts_with("PREPAY")
            || matches!(code.as_str(), "现结" | "预付款" | "先款")
        {
            return Ok(approved_on);
        }
        let days = match code.as_str() {
            "POSTPAY_NET15" | "POSTPAY-NET15" | "NET15" | "NET-15" => 15,
            "POSTPAY_NET30" | "POSTPAY-NET30" | "NET30" | "NET-30" => 30,
            _ => return Err(Error::from("付款条件未登记计划付款日规则")),
        };
        let delivery_on = expected_delivery_on.ok_or("后付条件缺少预计交付日")?;
        let due = delivery_on
            .as_naive_date()
            .checked_add_days(Days::new(days))
            .ok_or("计划付款日超出支持范围")?;
        BusinessDate::from_ymd(due.year(), due.month(), due.day())
            .ok_or_else(|| Error::from("计划付款日无效"))
    }
}

#[cfg(test)]
mod tests {
    use super::{PaymentTermSnapshot, SupplierSnapshot};
    use crate::common::time::BusinessDate;

    #[test]
    fn supplier_snapshot_trims_and_requires_name() {
        let snapshot = SupplierSnapshot::new(" 北京华联供应商 ".to_string()).unwrap();
        assert_eq!(snapshot.supplier_name, "北京华联供应商");
        assert!(SupplierSnapshot::new("   ".to_string()).is_err());
    }

    #[test]
    fn payment_term_snapshot_normalizes_and_rejects_negative_gates() {
        use crate::money::Amount;
        use std::str::FromStr;

        let snapshot = PaymentTermSnapshot::new(
            " PREPAY-30 ".to_string(),
            true,
            Some(Amount::from_str("100.00").unwrap()),
            None,
        );
        assert!(snapshot.is_ok());
        assert_eq!(snapshot.unwrap().payment_term_code, "PREPAY-30");

        let negative = PaymentTermSnapshot::new(
            "P30".to_string(),
            true,
            Some(Amount::from_str("-1.00").unwrap()),
            None,
        );
        assert!(negative.is_err());
    }

    #[test]
    fn payment_term_computes_prepay_and_postpay_due_dates() {
        let approved = BusinessDate::from_ymd(2026, 8, 26).unwrap();
        let delivery = BusinessDate::from_ymd(2026, 9, 1).unwrap();
        let prepay = PaymentTermSnapshot::new("PREPAY_30".to_string(), true, None, None).unwrap();
        let postpay = PaymentTermSnapshot::new("POSTPAY_NET15".to_string(), false, None, None).unwrap();

        assert_eq!(prepay.payable_due_date(approved, None).unwrap(), approved);
        assert_eq!(
            postpay.payable_due_date(approved, Some(delivery)).unwrap(),
            BusinessDate::from_ymd(2026, 9, 16).unwrap()
        );
    }

    #[test]
    fn payment_term_fails_closed_without_a_registered_due_rule() {
        let approved = BusinessDate::from_ymd(2026, 8, 26).unwrap();
        let postpay = PaymentTermSnapshot::new("NET-30".to_string(), false, None, None).unwrap();
        let unknown = PaymentTermSnapshot::new("先用后付".to_string(), false, None, None).unwrap();

        assert!(postpay.payable_due_date(approved, None).is_err());
        assert!(unknown.payable_due_date(approved, Some(approved)).is_err());
    }
}
