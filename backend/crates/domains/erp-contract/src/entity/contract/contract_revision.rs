//! `contract_revision`：合同不可变版本（数据模型 §6.4 / W04）。
//!
//! 每个版本恰好关联一份已签署合同正文 PDF；版本一经形成不得修改，只允许追加更高
//! 序号的新版本（数据模型 §4.3 不可变修订）。版本内联合同编号、客户名称、结算
//! 主体、税务与付款条件等结构化快照（P1 §2.2）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::revision::RevisionBase;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{ContractId, ContractRevisionId, CustomerAccountId, FileAssetId, PartyId};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::Contract;
use super::snapshot::{
    ContractSnapshot, CustomerSnapshot, InvoiceRequirementSnapshot, PaymentTermSnapshot,
    SettlementPartySnapshot,
};
use crate::entity::recognition::RecognitionProof;

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
    /// 返回合同归档来源的中文名称，供业务界面展示，不作为持久化代码。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ContractCenter => "合同中心",
            Self::SalesOrderCreate => "销售建单",
        }
    }

    /// 返回合同归档来源的稳定代码，供持久化与查询匹配使用。
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
    /// 自动识别与主数据匹配证据；历史归档无此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognition: Option<RecognitionProof>,
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
    /// 归档时用合同稳定身份、聚合内递增的修订号和 PDF 文件资产创建不可变版本，冻结客户、结算、付款及开票资料。
    /// 修订号为零、编号或快照为空/过长，或者到期日不晚于开始日时拒绝。
    pub fn new(
        id: ContractRevisionId,
        contract_id: ContractId,
        revision_no: u32,
        data: ContractRevisionData,
    ) -> Result<Self> {
        if revision_no == 0 {
            return Err(Error::from("合同版本号必须为正整数"));
        }
        let contract_no = ContractSnapshot::new(data.contract_no)?.contract_no;
        if let Some(valid_to) = data.valid_to
            && valid_to <= data.valid_from
        {
            return Err(Error::from("合同有效期止必须晚于有效期起"));
        }

        Ok(Self {
            recognition: None,
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

    /// 已归档的合同修订不允许覆盖；此入口始终返回不可变修订错误。业务变更须追加新修订。
    pub fn update(&mut self, _data: ContractRevisionData) -> Result<()> {
        Err(Error::from("合同版本是不可变修订，不支持更新"))
    }

    /// 首次归档或追加修订时，以历史最大修订号加一；无历史传 `0` 得到 `1`，达到 `u32::MAX` 时返回错误。
    pub fn next_revision_no(current_max: u32) -> Result<u32> {
        current_max.checked_add(1).ok_or_else(|| Error::from("合同版本号溢出"))
    }

    /// 解析销售或归档引用时，只比较本修订与所选稳定合同的身份，不查询活动版本或结算主体。
    pub fn belongs_to_contract(&self, contract_id: &ContractId) -> bool {
        &self.contract_id == contract_id
    }

    /// 销售引用既有归档修订时，核对实际客户、稳定合同身份、合同编号及引用方冻结的编号一致。
    /// 任一不匹配或引用方缺少编号均拒绝；结算主体由销售单另行约定。
    pub fn ensure_customer_reference(
        &self,
        contract: &Contract,
        customer_id: &CustomerAccountId,
        contract_no: Option<&str>,
    ) -> Result<()> {
        if self.contract_id.as_ref() != contract.base.id.as_str()
            || self.contract_no != contract.contract_no
            || &contract.customer_id != customer_id
            || contract_no != Some(self.contract_no.as_str())
        {
            return Err(Error::from("合同引用的客户、身份或编号不匹配"));
        }
        Ok(())
    }

    /// 核对引用身份时，只比较修订快照的结算主体与调用方给出的合同当前主体；不读取其他聚合，主体漂移由调用方映射为冲突。
    pub fn matches_settlement_party(&self, settlement_party_id: &PartyId) -> bool {
        &self.settlement_party_id == settlement_party_id
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::ContractId;

    use super::*;
    use crate::ContractData;

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
        let revision =
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, data())
                .unwrap();

        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.contract_id, ContractId::new("c-1"));
        assert_eq!(revision.contract_no, "HT-2026-0088");
        assert_eq!(revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(revision.settlement_party_snapshot.settlement_party_name, "集团结算中心");
        assert_eq!(revision.payment_term_snapshot.payment_term_name, "月结 30 天");
        assert_eq!(revision.invoice_requirement_snapshot.tax_point, "6");
        assert_eq!(revision.contract_pdf_file_id, FileAssetId::new("file-1"));
    }

    #[test]
    fn new_rejects_blank_and_reversed_validity() {
        let blank_no = ContractRevisionData { contract_no: "   ".to_string(), ..data() };
        assert!(
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, blank_no)
                .is_err()
        );

        let reversed =
            ContractRevisionData { valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()), ..data() };
        assert!(
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, reversed)
                .is_err()
        );

        assert!(
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 0, data())
                .is_err()
        );
    }

    #[test]
    fn update_is_rejected_for_immutable_revision() {
        let mut revision =
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, data())
                .unwrap();
        assert!(revision.update(data()).is_err());
    }

    #[test]
    fn belongs_to_contract_and_settlement_party_detect_drift() {
        let revision =
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, data())
                .unwrap();

        assert!(revision.belongs_to_contract(&ContractId::new("c-1")));
        assert!(!revision.belongs_to_contract(&ContractId::new("c-other")));
        assert!(revision.matches_settlement_party(&PartyId::new("party-1")));
        assert!(!revision.matches_settlement_party(&PartyId::new("party-other")));
    }

    #[test]
    fn customer_reference_keeps_historical_contract_identity_independent_of_settlement() {
        let revision =
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, data())
                .unwrap();
        let mut contract = Contract::new(
            ContractId::new("c-1"),
            ContractData {
                contract_no: revision.contract_no.clone(),
                customer_id: CustomerAccountId::new("customer-1"),
                settlement_party_id: PartyId::new("another-settlement"),
            },
            "sales",
        )
        .unwrap();
        contract.attach_revision("rev-2", "sales");
        contract.terminate("sales").unwrap();

        revision
            .ensure_customer_reference(&contract, &CustomerAccountId::new("customer-1"), Some("HT-2026-0088"))
            .unwrap();

        assert!(
            revision
                .ensure_customer_reference(
                    &contract,
                    &CustomerAccountId::new("customer-other"),
                    Some("HT-2026-0088"),
                )
                .is_err()
        );
        for number in [None, Some("HT-OTHER")] {
            assert!(
                revision
                    .ensure_customer_reference(&contract, &CustomerAccountId::new("customer-1"), number)
                    .is_err()
            );
        }

        contract.base.id = "contract-other".into();
        assert!(
            revision
                .ensure_customer_reference(
                    &contract,
                    &CustomerAccountId::new("customer-1"),
                    Some("HT-2026-0088"),
                )
                .is_err()
        );

        contract.base.id = "c-1".into();
        contract.contract_no = "HT-OTHER".into();
        assert!(
            revision
                .ensure_customer_reference(
                    &contract,
                    &CustomerAccountId::new("customer-1"),
                    Some("HT-2026-0088"),
                )
                .is_err()
        );
    }

    #[test]
    fn next_revision_no_is_checked() {
        assert_eq!(ContractRevision::next_revision_no(0).unwrap(), 1);
        assert_eq!(ContractRevision::next_revision_no(7).unwrap(), 8);
        assert_eq!(ContractRevision::next_revision_no(u32::MAX).unwrap_err().to_string(), "合同版本号溢出");
    }

    #[test]
    fn overlong_snapshot_rejected() {
        let overlong = ContractRevisionData { customer_name: "x".repeat(129), ..data() };
        assert!(
            ContractRevision::new(ContractRevisionId::new("rev-1"), ContractId::new("c-1"), 1, overlong)
                .is_err()
        );
    }
}
