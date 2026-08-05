//! 销售单域结构化快照值对象（数据模型 §4.4 / P1 §2.2）。
//!
//! 正式版本（`sales_order_revision`）与提交/工作副本必须内联当时的客户名称、合同
//! 编号、结算主体、税务与付款条件等**结构化**快照，后续基础资料修改不改变历史
//! 单据；禁止用 JSON blob 承载（数据模型 §4.4）。
//!
//! 本组类型是 D12/D13/D14/D15 共用的同形值对象；因 `common/**` 在 P0 冻结，
//! 各域各自定义（P1 合同 §3 跨域约束），待 `chore/erp-p0-amend-*` 地基修订统一
//! 下沉到 `entities/src/common/`。

use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::validation::{normalize_optional_text, normalize_required_text};

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

/// 客户名称快照（一期来源 `company_name` 的结构化落点）。
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

/// 表头结构化快照创建入参（原始字符串，由 [`HeaderSnapshots::build`] 统一规范化）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderSnapshotData {
    /// 客户名称。
    pub customer_name: String,
    /// 合同编号；无合同时为 `None`。
    pub contract_no: Option<String>,
    /// 结算主体名称；与 `settlement_party_id` 同时提供。
    pub settlement_party_name: Option<String>,
    /// 付款条件代码。
    pub payment_term_code: String,
    /// 付款条件名称。
    pub payment_term_name: String,
    /// 开票类型。
    pub invoice_type: String,
    /// 税点。
    pub tax_point: String,
}

impl Default for HeaderSnapshotData {
    /// 空表头快照（供测试与占位使用；实体 `new` 仍会校验非空字段）。
    fn default() -> Self {
        Self {
            customer_name: String::new(),
            contract_no: None,
            settlement_party_name: None,
            payment_term_code: String::new(),
            payment_term_name: String::new(),
            invoice_type: String::new(),
            tax_point: String::new(),
        }
    }
}

/// 表头结构化快照集合（以 `#[serde(flatten)]` 平铺进实体，字段名与字段字典一致）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderSnapshots {
    /// 客户名称快照。
    pub customer_snapshot: CustomerSnapshot,
    /// 合同编号快照。
    pub contract_snapshot: Option<ContractSnapshot>,
    /// 结算主体名称快照。
    pub settlement_party_snapshot: Option<SettlementPartySnapshot>,
    /// 结构化付款条件快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 结构化开票要求快照。
    pub invoice_requirement_snapshot: InvoiceRequirementSnapshot,
}

impl HeaderSnapshots {
    /// 从原始入参构建快照集合（统一 trim、非空与长度上限校验）。
    ///
    /// # 参数
    /// * `data` - 表头快照入参
    ///
    /// # 返回
    /// 返回规范化后的快照集合。
    ///
    /// # 错误
    /// 任一必填快照为空或超长时返回错误。
    pub fn build(data: &HeaderSnapshotData) -> Result<Self> {
        let contract_snapshot =
            normalize_optional_text(data.contract_no.clone(), "合同编号", CONTRACT_NO_MAX_LEN)?
                .map(ContractSnapshot::new)
                .transpose()?;
        let settlement_party_snapshot =
            normalize_optional_text(data.settlement_party_name.clone(), "结算主体", PARTY_NAME_MAX_LEN)?
                .map(SettlementPartySnapshot::new)
                .transpose()?;

        Ok(Self {
            customer_snapshot: CustomerSnapshot::new(data.customer_name.clone())?,
            contract_snapshot,
            settlement_party_snapshot,
            payment_term_snapshot: PaymentTermSnapshot::new(
                data.payment_term_code.clone(),
                data.payment_term_name.clone(),
            )?,
            invoice_requirement_snapshot: InvoiceRequirementSnapshot::new(
                data.invoice_type.clone(),
                data.tax_point.clone(),
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_trim_and_validate() {
        let customer = CustomerSnapshot::new(" 东方企业 ".to_string()).unwrap();
        assert_eq!(customer.customer_name, "东方企业");

        let contract = ContractSnapshot::new(" HT-2026-0088 ".to_string()).unwrap();
        assert_eq!(contract.contract_no, "HT-2026-0088");

        let term = PaymentTermSnapshot::new("NET30".to_string(), " 月结 30 天 ".to_string()).unwrap();
        assert_eq!(term.payment_term_name, "月结 30 天");

        let invoice =
            InvoiceRequirementSnapshot::new(" 增值税专用发票 ".to_string(), " 6 ".to_string()).unwrap();
        assert_eq!(invoice.tax_point, "6");
    }

    #[test]
    fn snapshots_reject_blank_and_overlong() {
        assert!(CustomerSnapshot::new("   ".to_string()).is_err());
        assert!(ContractSnapshot::new("x".repeat(65)).is_err());
        assert!(SettlementPartySnapshot::new("x".repeat(129)).is_err());
        assert!(InvoiceRequirementSnapshot::new("type".to_string(), "  ".to_string()).is_err());
    }

    #[test]
    fn header_snapshots_build_normalizes_all_fields() {
        let snapshots = HeaderSnapshots::build(&HeaderSnapshotData {
            customer_name: " 东方企业 ".to_string(),
            contract_no: Some(" HT-2026-0088 ".to_string()),
            settlement_party_name: Some(" 集团结算中心 ".to_string()),
            payment_term_code: "NET30".to_string(),
            payment_term_name: " 月结 30 天 ".to_string(),
            invoice_type: " 增值税专用发票 ".to_string(),
            tax_point: " 6 ".to_string(),
        })
        .unwrap();

        assert_eq!(snapshots.customer_snapshot.customer_name, "东方企业");
        assert_eq!(snapshots.contract_snapshot.unwrap().contract_no, "HT-2026-0088");
        assert_eq!(
            snapshots.settlement_party_snapshot.unwrap().settlement_party_name,
            "集团结算中心"
        );
        assert_eq!(snapshots.payment_term_snapshot.payment_term_name, "月结 30 天");
        assert_eq!(
            snapshots.invoice_requirement_snapshot.invoice_type,
            "增值税专用发票"
        );
    }

    #[test]
    fn header_snapshots_build_rejects_blank_mandatory_fields() {
        let blank = HeaderSnapshotData {
            customer_name: "   ".to_string(),
            ..Default::default()
        };
        assert!(HeaderSnapshots::build(&blank).is_err());
    }
}
