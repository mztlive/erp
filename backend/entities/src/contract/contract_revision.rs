//! `contract_revision`：合同不可变版本（数据模型 §6.4 / W04）。
//!
//! 每个版本恰好关联一份已签署合同正文 PDF；版本一经形成不得修改，只允许追加更高
//! 序号的新版本（数据模型 §4.3 不可变修订）。版本内联合同编号、客户名称、结算
//! 主体、税务与付款条件等结构化快照（P1 §2.2）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{ContractId, ContractRevisionId, FileAssetId, PartyId};
use crate::validation::normalize_required_text;

use super::snapshot::{
    CustomerSnapshot, InvoiceRequirementSnapshot, PaymentTermSnapshot, SettlementPartySnapshot,
};

/// 合同编号最大长度。
const CONTRACT_NO_MAX_LEN: usize = 64;

/// 合同归档来源（数据模型 §6.4：`CONTRACT_CENTER`、`SALES_ORDER_CREATE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArchiveSource {
    /// 合同中心（W04）上传归档。
    ContractCenter,
    /// 销售建单页（W05）旁挂上传归档。
    SalesOrderCreate,
}

impl ArchiveSource {
    /// 返回来源的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ContractCenter => "合同中心",
            Self::SalesOrderCreate => "销售建单",
        }
    }

    /// 返回来源的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContractCenter => "CONTRACT_CENTER",
            Self::SalesOrderCreate => "SALES_ORDER_CREATE",
        }
    }
}

/// 合同版本创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRevisionData {
    /// 合同编号（版本内联快照，与主表一致）。
    pub contract_no: String,
    /// 客户名称快照。
    pub customer_name: String,
    /// 本版本已签署合同 PDF（每个版本恰好一份正文 PDF）。
    pub contract_pdf_file_id: FileAssetId,
    /// 归档来源。
    pub archive_source: ArchiveSource,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 结算主体名称快照。
    pub settlement_party_name: String,
    /// 付款条件代码（结构化快照）。
    pub payment_term_code: String,
    /// 付款条件名称（结构化快照）。
    pub payment_term_name: String,
    /// 开票类型（结构化快照）。
    pub invoice_type: String,
    /// 税点（结构化快照）。
    pub tax_point: String,
    /// 合同有效期起。
    pub valid_from: BusinessDate,
    /// 合同有效期止；`None` 表示长期合同无固定到期日。
    pub valid_to: Option<BusinessDate>,
    /// 签订日期。
    pub signed_at: BusinessDate,
}

/// 合同版本实体（不可变修订，数据模型 §6.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ContractRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属合同。
    pub contract_id: ContractId,
    /// 合同编号（版本内联快照）。
    pub contract_no: String,
    /// 客户名称快照。
    pub customer_snapshot: CustomerSnapshot,
    /// 本版本已签署合同 PDF。
    pub contract_pdf_file_id: FileAssetId,
    /// 归档来源。
    pub archive_source: ArchiveSource,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 结算主体名称快照。
    pub settlement_party_snapshot: SettlementPartySnapshot,
    /// 结构化付款条件快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 结构化开票要求快照。
    pub invoice_requirement_snapshot: InvoiceRequirementSnapshot,
    /// 合同有效期起。
    pub valid_from: BusinessDate,
    /// 合同有效期止。
    pub valid_to: Option<BusinessDate>,
    /// 签订日期。
    pub signed_at: BusinessDate,
}

impl ContractRevision {
    /// 创建合同版本（不可变）。
    ///
    /// 完成 contract_no 与全部快照字段的校验与规范化，并强制有效期不变式：
    /// `valid_to` 必须晚于 `valid_from`。版本一经形成不允许更新。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ContractRevisionId`）
    /// * `contract_id` - 所属合同主键
    /// * `revision_no` - 聚合内从 1 递增的版本号
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的合同版本实体。
    ///
    /// # 错误
    /// 编号/快照为空、超长，或有效期倒挂时返回错误。
    pub fn new(
        id: ContractRevisionId,
        contract_id: ContractId,
        revision_no: u32,
        data: ContractRevisionData,
    ) -> Result<Self> {
        if revision_no == 0 {
            return Err(Error::from("合同版本号必须为正整数"));
        }
        let contract_no = normalize_required_text(
            data.contract_no,
            "合同编号不能为空",
            CONTRACT_NO_MAX_LEN,
            "合同编号过长",
        )?;
        if let Some(valid_to) = data.valid_to {
            if valid_to <= data.valid_from {
                return Err(Error::from("合同有效期止必须晚于有效期起"));
            }
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(revision_no),
            contract_id,
            contract_no,
            customer_snapshot: CustomerSnapshot::new(data.customer_name)?,
            contract_pdf_file_id: data.contract_pdf_file_id,
            archive_source: data.archive_source,
            settlement_party_id: data.settlement_party_id,
            settlement_party_snapshot: SettlementPartySnapshot::new(data.settlement_party_name)?,
            payment_term_snapshot: PaymentTermSnapshot::new(data.payment_term_code, data.payment_term_name)?,
            invoice_requirement_snapshot: InvoiceRequirementSnapshot::new(data.invoice_type, data.tax_point)?,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            signed_at: data.signed_at,
        })
    }

    /// 更新合同版本。
    ///
    /// 合同版本是正式归档的不可变修订（数据模型 §4.3），业务字段不允许更新。
    ///
    /// # 参数
    /// * `_data` - 更新数据（被拒绝，保留签名以表达不可变性契约）
    ///
    /// # 返回
    /// 恒返回 `Err`。
    ///
    /// # 错误
    /// 恒返回「不可变版本不支持更新」错误。
    pub fn update(&mut self, _data: ContractRevisionData) -> Result<()> {
        Err(Error::from("合同版本是不可变修订，不支持更新"))
    }

    /// 由当前最大修订序号计算下一合同修订序号。
    ///
    /// # 参数
    /// * `current_max` - 当前合同历史最大修订序号；没有历史时为 `0`
    ///
    /// # 返回
    /// 返回严格递增的下一修订序号。
    ///
    /// # 错误
    /// 当前修订序号达到 `u32::MAX` 时返回错误。
    pub fn next_revision_no(current_max: u32) -> Result<u32> {
        current_max
            .checked_add(1)
            .ok_or_else(|| Error::from("合同版本号溢出"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::time::BusinessDate;
    use crate::ids::ContractId;

    fn data() -> ContractRevisionData {
        ContractRevisionData {
            contract_no: " HT-2026-0088 ".to_string(),
            customer_name: " 东方企业 ".to_string(),
            contract_pdf_file_id: FileAssetId::new("file-1"),
            archive_source: ArchiveSource::ContractCenter,
            settlement_party_id: PartyId::new("party-1"),
            settlement_party_name: " 集团结算中心 ".to_string(),
            payment_term_code: "NET30".to_string(),
            payment_term_name: " 月结 30 天 ".to_string(),
            invoice_type: " 增值税专用发票 ".to_string(),
            tax_point: " 6 ".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            signed_at: BusinessDate::from_ymd(2025, 12, 20).unwrap(),
        }
    }

    #[test]
    fn new_trims_snapshots_and_keeps_revision_metadata() {
        let revision = ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            1,
            data(),
        )
        .unwrap();

        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.contract_id, ContractId::new("c-1"));
        assert_eq!(revision.contract_no, "HT-2026-0088");
        assert_eq!(revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(
            revision.settlement_party_snapshot.settlement_party_name,
            "集团结算中心"
        );
        assert_eq!(revision.payment_term_snapshot.payment_term_name, "月结 30 天");
        assert_eq!(revision.invoice_requirement_snapshot.tax_point, "6");
        assert_eq!(revision.contract_pdf_file_id, FileAssetId::new("file-1"));
    }

    #[test]
    fn new_rejects_blank_and_reversed_validity() {
        let blank_no = ContractRevisionData {
            contract_no: "   ".to_string(),
            ..data()
        };
        assert!(ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            1,
            blank_no
        )
        .is_err());

        let reversed = ContractRevisionData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..data()
        };
        assert!(ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            1,
            reversed
        )
        .is_err());

        assert!(ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            0,
            data()
        )
        .is_err());
    }

    #[test]
    fn update_is_rejected_for_immutable_revision() {
        let mut revision = ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            1,
            data(),
        )
        .unwrap();
        assert!(revision.update(data()).is_err());
    }

    #[test]
    fn next_revision_no_is_checked() {
        assert_eq!(ContractRevision::next_revision_no(0).unwrap(), 1);
        assert_eq!(ContractRevision::next_revision_no(7).unwrap(), 8);
        assert_eq!(
            ContractRevision::next_revision_no(u32::MAX)
                .unwrap_err()
                .to_string(),
            "合同版本号溢出"
        );
    }

    #[test]
    fn overlong_snapshot_rejected() {
        let overlong = ContractRevisionData {
            customer_name: "x".repeat(129),
            ..data()
        };
        assert!(ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            1,
            overlong
        )
        .is_err());
    }
}
