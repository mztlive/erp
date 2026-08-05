//! 合同域结构化快照值对象（数据模型 §4.4 / P1 §2.2）。
//!
//! 正式版本（`*_revision`）必须内联当时的客户名称、合同编号、结算主体、税务与
//! 付款条件等**结构化**快照字段，后续基础资料修改不改变历史单据；禁止用 JSON blob
//! 承载（数据模型 §4.4）。
//!
//! 本组类型是 D12/D13/D14/D15 共用的同形值对象；因 `common/**` 在 P0 冻结，
//! 各域各自定义（P1 合同 §3 跨域约束），待 `chore/erp-p0-amend-*` 地基修订统一
//! 下沉到 `entities/src/common/`。

use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::validation::normalize_required_text;

/// 客户名称最大长度。
const CUSTOMER_NAME_MAX_LEN: usize = 128;
/// 合同编号最大长度。
const CONTRACT_NO_MAX_LEN: usize = 64;
/// 主体名称最大长度。
const PARTY_NAME_MAX_LEN: usize = 128;
/// 付款条件代码最大长度。
const PAYMENT_TERM_CODE_MAX_LEN: usize = 32;
/// 付款条件名称最大长度。
const PAYMENT_TERM_NAME_MAX_LEN: usize = 64;
/// 开票类型最大长度。
const INVOICE_TYPE_MAX_LEN: usize = 32;
/// 税点最大长度。
const TAX_POINT_MAX_LEN: usize = 16;

/// 客户名称快照（销售单客户快照与合同版本客户快照共用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerSnapshot {
    /// 客户名称。
    pub customer_name: String,
}

impl CustomerSnapshot {
    /// 构造客户名称快照（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `customer_name` - 客户名称
    ///
    /// # 返回
    /// 返回规范化后的快照。
    ///
    /// # 错误
    /// 名称为空或超长时返回错误。
    pub fn new(customer_name: impl Into<String>) -> Result<Self> {
        let customer_name = normalize_required_text(
            customer_name.into(),
            "客户名称不能为空",
            CUSTOMER_NAME_MAX_LEN,
            "客户名称过长",
        )?;
        Ok(Self { customer_name })
    }
}

/// 合同编号快照（历史销售版本固定引用当时的合同编号）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractSnapshot {
    /// 合同编号。
    pub contract_no: String,
}

impl ContractSnapshot {
    /// 构造合同编号快照（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `contract_no` - 合同编号
    ///
    /// # 返回
    /// 返回规范化后的快照。
    ///
    /// # 错误
    /// 编号为空或超长时返回错误。
    pub fn new(contract_no: impl Into<String>) -> Result<Self> {
        let contract_no = normalize_required_text(
            contract_no.into(),
            "合同编号不能为空",
            CONTRACT_NO_MAX_LEN,
            "合同编号过长",
        )?;
        Ok(Self { contract_no })
    }
}

/// 结算主体名称快照（与 `settlement_party_id` 同时保存，展示不依赖当前主体资料）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementPartySnapshot {
    /// 结算主体名称。
    pub settlement_party_name: String,
}

impl SettlementPartySnapshot {
    /// 构造结算主体快照（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `settlement_party_name` - 结算主体名称
    ///
    /// # 返回
    /// 返回规范化后的快照。
    ///
    /// # 错误
    /// 名称为空或超长时返回错误。
    pub fn new(settlement_party_name: impl Into<String>) -> Result<Self> {
        let settlement_party_name = normalize_required_text(
            settlement_party_name.into(),
            "结算主体名称不能为空",
            PARTY_NAME_MAX_LEN,
            "结算主体名称过长",
        )?;
        Ok(Self {
            settlement_party_name,
        })
    }
}

/// 结构化付款条件快照（数据模型 §6.4：码表结构化快照，不自由长文本录入）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentTermSnapshot {
    /// 付款条件代码。
    pub payment_term_code: String,
    /// 付款条件名称。
    pub payment_term_name: String,
}

impl PaymentTermSnapshot {
    /// 构造付款条件快照（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `payment_term_code` - 付款条件代码
    /// * `payment_term_name` - 付款条件名称
    ///
    /// # 返回
    /// 返回规范化后的快照。
    ///
    /// # 错误
    /// 代码或名称为空、超长时返回错误。
    pub fn new(payment_term_code: impl Into<String>, payment_term_name: impl Into<String>) -> Result<Self> {
        let payment_term_code = normalize_required_text(
            payment_term_code.into(),
            "付款条件代码不能为空",
            PAYMENT_TERM_CODE_MAX_LEN,
            "付款条件代码过长",
        )?;
        let payment_term_name = normalize_required_text(
            payment_term_name.into(),
            "付款条件名称不能为空",
            PAYMENT_TERM_NAME_MAX_LEN,
            "付款条件名称过长",
        )?;
        Ok(Self {
            payment_term_code,
            payment_term_name,
        })
    }
}

/// 结构化开票要求快照（数据模型 §6.4：开票类型与税点，即「税务」快照）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceRequirementSnapshot {
    /// 开票类型。
    pub invoice_type: String,
    /// 税点（如 `6`、`0.06` 的来源枚举值原样保存）。
    pub tax_point: String,
}

impl InvoiceRequirementSnapshot {
    /// 构造开票要求快照（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `invoice_type` - 开票类型
    /// * `tax_point` - 税点
    ///
    /// # 返回
    /// 返回规范化后的快照。
    ///
    /// # 错误
    /// 类型或税点为空、超长时返回错误。
    pub fn new(invoice_type: impl Into<String>, tax_point: impl Into<String>) -> Result<Self> {
        let invoice_type = normalize_required_text(
            invoice_type.into(),
            "开票类型不能为空",
            INVOICE_TYPE_MAX_LEN,
            "开票类型过长",
        )?;
        let tax_point =
            normalize_required_text(tax_point.into(), "税点不能为空", TAX_POINT_MAX_LEN, "税点过长")?;
        Ok(Self {
            invoice_type,
            tax_point,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContractSnapshot, CustomerSnapshot, InvoiceRequirementSnapshot, PaymentTermSnapshot,
        SettlementPartySnapshot,
    };

    #[test]
    fn snapshots_trim_and_validate() {
        let customer = CustomerSnapshot::new(" 东方企业 ".to_string()).unwrap();
        assert_eq!(customer.customer_name, "东方企业");

        let contract = ContractSnapshot::new(" HT-2026-0088 ".to_string()).unwrap();
        assert_eq!(contract.contract_no, "HT-2026-0088");

        let party = SettlementPartySnapshot::new(" 集团结算中心 ".to_string()).unwrap();
        assert_eq!(party.settlement_party_name, "集团结算中心");

        let term = PaymentTermSnapshot::new("NET30".to_string(), " 月结 30 天 ".to_string()).unwrap();
        assert_eq!(term.payment_term_code, "NET30");
        assert_eq!(term.payment_term_name, "月结 30 天");

        let invoice =
            InvoiceRequirementSnapshot::new(" 增值税专用发票 ".to_string(), " 6 ".to_string()).unwrap();
        assert_eq!(invoice.invoice_type, "增值税专用发票");
        assert_eq!(invoice.tax_point, "6");
    }

    #[test]
    fn snapshots_reject_blank_and_overlong() {
        assert!(CustomerSnapshot::new("   ".to_string()).is_err());
        assert!(ContractSnapshot::new("x".repeat(65)).is_err());
        assert!(SettlementPartySnapshot::new("x".repeat(129)).is_err());
        assert!(PaymentTermSnapshot::new("  ".to_string(), "name".to_string()).is_err());
        assert!(InvoiceRequirementSnapshot::new("type".to_string(), "x".repeat(17)).is_err());
    }
}
