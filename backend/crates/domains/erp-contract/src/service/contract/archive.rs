//! 合同归档规划：纯规则装配尚未持久化的合同与不可变修订。
//!
//! 三类归档入口（首次、一次上传、追加版本）共用合同创建与版本组装，
//! 本模块只做字段搬运与序号规则，不触碰数据库或外部端口。

use erp_core::common::time::BusinessDate;
use erp_core::ids::{ContractId, ContractRevisionId, FileAssetId, PartyId};
use id_generator::next_id;

use crate::dto::contract::{ArchiveContractRevisionRequest, CreateContractRequest, UploadContractRequest};
use crate::entity::contract::{
    ArchiveSource, Contract, ContractData, ContractRevision, ContractRevisionData,
};
use crate::error::{Error, Result};

/// 已规划、尚未持久化的合同身份与首个/下一个不可变修订。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedContractArchive {
    /// 合同稳定身份。
    pub contract: Contract,
    /// 不可变合同修订。
    pub revision: ContractRevision,
}

/// 三类归档规划请求共用的版本内联快照字段组。
pub(crate) struct ArchiveSnapshotFields {
    /// 客户名称快照。
    customer_name: String,
    /// 结算主体名称快照。
    settlement_party_name: String,
    /// 付款条件代码。
    payment_term_code: String,
    /// 付款条件名称。
    payment_term_name: String,
    /// 开票类型。
    invoice_type: String,
    /// 税点。
    tax_point: String,
    /// 合同有效期起。
    valid_from: BusinessDate,
    /// 合同有效期止；`None` 表示长期。
    valid_to: Option<BusinessDate>,
    /// 签订日期。
    signed_at: BusinessDate,
}

impl From<CreateContractRequest> for ArchiveSnapshotFields {
    fn from(req: CreateContractRequest) -> Self {
        Self {
            customer_name: req.customer_name,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        }
    }
}

impl From<UploadContractRequest> for ArchiveSnapshotFields {
    fn from(req: UploadContractRequest) -> Self {
        Self {
            customer_name: req.customer_name,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        }
    }
}

impl From<ArchiveContractRevisionRequest> for ArchiveSnapshotFields {
    fn from(req: ArchiveContractRevisionRequest) -> Self {
        Self {
            customer_name: req.customer_name,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        }
    }
}

// 归档规划共用的修订构造器：以稳定合同的编号和身份绑定指定修订序号、已分配的 PDF 文件资产以及当次结算和开票快照。
// 非法快照或有效期倒挂时拒绝，不写入数据库。
fn build_revision(
    contract: &Contract,
    contract_pdf_file_id: FileAssetId,
    archive_source: ArchiveSource,
    settlement_party_id: PartyId,
    snapshot: ArchiveSnapshotFields,
    revision_no: u32,
) -> Result<ContractRevision> {
    Ok(ContractRevision::new(
        ContractRevisionId::new(next_id()),
        contract.base.id.clone().into(),
        revision_no,
        ContractRevisionData {
            contract_no: contract.contract_no.clone(),
            customer_name: snapshot.customer_name,
            contract_pdf_file_id,
            archive_source,
            settlement_party_id,
            settlement_party_name: snapshot.settlement_party_name,
            payment_term_code: snapshot.payment_term_code,
            payment_term_name: snapshot.payment_term_name,
            invoice_type: snapshot.invoice_type,
            tax_point: snapshot.tax_point,
            valid_from: snapshot.valid_from,
            valid_to: snapshot.valid_to,
            signed_at: snapshot.signed_at,
        },
    )?)
}

/// 首次归档请求通过 `Validate` 后，用已登记的 PDF 文件资产身份和请求快照规划稳定合同及其首个不可变修订。
/// 返回未持久化的规划；合同编号或快照为空、超长或有效期倒挂时拒绝。
pub fn plan_first_archive(
    req: CreateContractRequest,
    actor_id: impl Into<String>,
) -> Result<PlannedContractArchive> {
    plan_initial_archive(
        ContractData {
            contract_no: req.contract_no.clone(),
            customer_id: req.customer_id.clone(),
            settlement_party_id: req.settlement_party_id.clone(),
        },
        req.contract_pdf_file_id.clone(),
        req.archive_source.unwrap_or(ArchiveSource::ContractCenter),
        req.into(),
        actor_id.into(),
    )
}

/// 上传归档已取得文件资产 ID 并补齐结算主体后，用请求快照规划稳定合同与首修订；不读取或写入 PDF 对象。
/// 返回未持久化的规划；合同编号或快照为空、超长或有效期倒挂时拒绝。
pub fn plan_upload_archive(
    req: UploadContractRequest,
    file_asset_id: FileAssetId,
    settlement_party_id: PartyId,
    actor_id: impl Into<String>,
) -> Result<PlannedContractArchive> {
    plan_initial_archive(
        ContractData {
            contract_no: req.contract_no.clone(),
            customer_id: req.customer_id.clone(),
            settlement_party_id,
        },
        file_asset_id,
        ArchiveSource::ContractCenter,
        req.into(),
        actor_id.into(),
    )
}

fn plan_initial_archive(
    data: ContractData,
    file_asset_id: FileAssetId,
    source: ArchiveSource,
    snapshot: ArchiveSnapshotFields,
    actor_id: String,
) -> Result<PlannedContractArchive> {
    let contract = Contract::new(ContractId::new(next_id()), data, actor_id)?;
    let revision =
        build_revision(&contract, file_asset_id, source, contract.settlement_party_id.clone(), snapshot, 1)?;
    Ok(PlannedContractArchive { contract, revision })
}

// 追加归档时从当前历史最大修订序号求下一个序号，保持合同稳定身份，将新 PDF 与当次快照绑定到不同修订。
// 序号溢出、快照非法或有效期倒挂时返回错误。
pub(crate) fn plan_next_revision(
    contract: Contract,
    req: ArchiveContractRevisionRequest,
    current_revision_no: u32,
) -> Result<PlannedContractArchive> {
    let next_no = ContractRevision::next_revision_no(current_revision_no)?;
    let revision = build_revision(
        &contract,
        req.contract_pdf_file_id.clone(),
        req.archive_source.unwrap_or(ArchiveSource::ContractCenter),
        contract.settlement_party_id.clone(),
        req.into(),
        next_no,
    )?;
    Ok(PlannedContractArchive { contract, revision })
}

// 归档前核对客户事实仍存在并启用。缺失映射 `NotFound`，停用映射 `BusinessLogicError`，保留原文案。
pub(crate) fn customer_eligibility(
    customer: Option<crate::ports::CustomerAccountFact>,
) -> Result<crate::ports::CustomerAccountFact> {
    let customer = customer.ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
    if !customer.is_active {
        return Err(Error::BusinessLogicError("客户已停用，禁止归档新合同".to_string()));
    }
    Ok(customer)
}

// 合同命令用实体的 `matches_version` 比较期望版本后，经此把不匹配统一映射为 `ConflictError`（HTTP 409）。
pub(crate) fn conflict_if_stale_version(matched: bool) -> Result<()> {
    if matched {
        return Ok(());
    }
    Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{FileAssetId, PartyId};
    use serde_json::json;

    use super::{plan_first_archive, plan_upload_archive};
    use crate::dto::contract::{CreateContractRequest, UploadContractRequest};
    use crate::entity::contract::ArchiveSource;

    fn archive_json() -> serde_json::Value {
        json!({
            "contract_no": "HT-2026-0088",
            "customer_id": "cust-1",
            "settlement_party_id": "party-1",
            "contract_pdf_file_id": "file-1",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "valid_to": "2026-12-31",
            "signed_at": "2025-12-20",
        })
    }

    #[test]
    fn plan_first_archive_keeps_request_snapshots_and_pdf_id() {
        let req: CreateContractRequest = serde_json::from_value(archive_json()).unwrap();
        let planned = plan_first_archive(req, "admin-1").unwrap();
        assert_eq!(planned.contract.contract_no, "HT-2026-0088");
        assert_eq!(planned.revision.revision.revision_no, 1);
        assert_eq!(planned.revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(planned.revision.settlement_party_snapshot.settlement_party_name, "集团结算中心");
        assert_eq!(planned.revision.contract_pdf_file_id, FileAssetId::new("file-1"));
        assert_eq!(planned.revision.archive_source, ArchiveSource::ContractCenter);
        assert_eq!(planned.revision.valid_from, BusinessDate::from_ymd(2026, 1, 1).unwrap());
    }

    #[test]
    fn plan_upload_archive_binds_assigned_file_id_and_default_source() {
        let req: UploadContractRequest = serde_json::from_value(json!({
            "contract_no": "HT-2026-0099",
            "customer_id": "cust-1",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "signed_at": "2025-12-20",
        }))
        .unwrap();
        let planned = plan_upload_archive(
            req,
            FileAssetId::new("asset-9"),
            PartyId::new("party-from-customer"),
            "admin-1",
        )
        .unwrap();

        assert_eq!(planned.contract.settlement_party_id, PartyId::new("party-from-customer"));
        assert_eq!(planned.revision.contract_pdf_file_id, FileAssetId::new("asset-9"));
        assert_eq!(planned.revision.archive_source, ArchiveSource::ContractCenter);
        assert_eq!(planned.revision.revision.revision_no, 1);
        assert_eq!(planned.revision.contract_id.as_ref(), planned.contract.base.id);
        assert_eq!(planned.revision.settlement_party_id, planned.contract.settlement_party_id);
    }

    #[test]
    fn next_revision_no_is_checked_and_sequenced() {
        use erp_core::ids::{CustomerAccountId, PartyId};

        use super::plan_next_revision;
        use crate::dto::contract::ArchiveContractRevisionRequest;
        use crate::entity::contract::{Contract, ContractData, ContractId};

        let contract = Contract::new(
            ContractId::new("c-1"),
            ContractData {
                contract_no: "HT-1".into(),
                customer_id: CustomerAccountId::new("cust-1"),
                settlement_party_id: PartyId::new("party-1"),
            },
            "admin-1",
        )
        .unwrap();
        let req: ArchiveContractRevisionRequest = serde_json::from_value(json!({
            "version": 2,
            "contract_pdf_file_id": "file-2",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "signed_at": "2025-12-20",
        }))
        .unwrap();
        let planned = plan_next_revision(contract, req, 1).unwrap();
        assert_eq!(planned.revision.revision.revision_no, 2);

        assert!(
            plan_next_revision(
                planned.contract,
                serde_json::from_value(json!({
                    "version": 2,
                    "contract_pdf_file_id": "file-2",
                    "customer_name": "东方企业",
                    "settlement_party_name": "集团结算中心",
                    "payment_term_code": "NET30",
                    "payment_term_name": "月结 30 天",
                    "invoice_type": "增值税专用发票",
                    "tax_point": "6",
                    "valid_from": "2026-01-01",
                    "signed_at": "2025-12-20",
                }))
                .unwrap(),
                u32::MAX
            )
            .is_err()
        );
    }
}
