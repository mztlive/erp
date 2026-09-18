//! 合同归档规划：纯规则装配尚未持久化的合同与不可变修订。
//!
//! 三类归档入口（首次、一次上传、追加版本）共用合同创建与版本组装，
//! 本模块只做字段搬运与序号规则，不触碰数据库或外部端口。

use erp_core::common::time::BusinessDate;
use erp_core::ids::{ContractId, ContractRevisionId, CustomerAccountId, FileAssetId, PartyId};
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

/// 由合同身份与快照组装不可变修订数据（三处规划函数共用）。
///
/// 版本号规则与默认来源回退由调用方完成，本函数只做字段搬运。
///
/// # 参数
/// * `contract` - 合同实体（提供编号与结算主体）
/// * `contract_pdf_file_id` - 本版本已签署合同 PDF 的文件资产
/// * `archive_source` - 归档来源（调用方已完成默认回退）
/// * `settlement_party_id` - 结算主体（调用方已按客户缺省补齐）
/// * `snapshot` - 归档快照字段组
///
/// # 返回
/// 返回尚未持久化的修订创建数据。
fn revision_data(
    contract: &Contract,
    contract_pdf_file_id: FileAssetId,
    archive_source: ArchiveSource,
    settlement_party_id: PartyId,
    snapshot: ArchiveSnapshotFields,
) -> ContractRevisionData {
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
    }
}

/// 创建合同稳定身份（三处规划函数共用 ID 分配与实体校验）。
///
/// # 参数
/// * `contract_no` - 合同编号
/// * `customer_id` - 客户稳定身份
/// * `settlement_party_id` - 结算主体
/// * `actor_id` - 创建人
///
/// # 返回
/// 返回新建的合同实体。
///
/// # 错误
/// 编号为空、超长时返回错误。
fn new_contract(
    contract_no: String,
    customer_id: CustomerAccountId,
    settlement_party_id: PartyId,
    actor_id: String,
) -> Result<Contract> {
    Ok(Contract::new(
        ContractId::new(next_id()),
        ContractData { contract_no, customer_id, settlement_party_id },
        actor_id,
    )?)
}

/// 组装指定序号的不可变修订（三处规划函数共用版本落盘形状）。
///
/// # 参数
/// * `contract` - 所属合同（提供编号与默认结算主体）
/// * `contract_pdf_file_id` - 本版本 PDF 文件资产
/// * `archive_source` - 归档来源
/// * `settlement_party_id` - 结算主体
/// * `snapshot` - 归档快照字段组
/// * `revision_no` - 聚合内版本号
///
/// # 返回
/// 返回新建的合同版本实体。
///
/// # 错误
/// 快照非法或有效期倒挂时返回错误。
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
        revision_data(contract, contract_pdf_file_id, archive_source, settlement_party_id, snapshot),
    )?)
}

/// 由首次归档请求构造合同身份与首个不可变修订。
///
/// # 参数
/// * `req` - 已通过 `Validate` 的创建请求
/// * `actor_id` - 创建人
///
/// # 返回
/// 返回尚未持久化的合同与修订。
///
/// # 错误
/// 编号/快照为空、超长或有效期倒挂。
pub fn plan_first_archive(
    req: CreateContractRequest,
    actor_id: impl Into<String>,
) -> Result<PlannedContractArchive> {
    let actor_id = actor_id.into();
    let contract = new_contract(req.contract_no, req.customer_id, req.settlement_party_id, actor_id)?;
    let revision = build_revision(
        &contract,
        req.contract_pdf_file_id,
        req.archive_source.unwrap_or(ArchiveSource::ContractCenter),
        contract.settlement_party_id.clone(),
        ArchiveSnapshotFields {
            customer_name: req.customer_name,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        },
        1,
    )?;
    Ok(PlannedContractArchive { contract, revision })
}

/// 由一次上传命令构造合同身份与首个不可变修订。
///
/// # 参数
/// * `req` - 已通过 `Validate` 的上传命令
/// * `file_asset_id` - 已分配的文件资产 ID
/// * `settlement_party_id` - 结算主体；调用方已按客户缺省补齐
/// * `actor_id` - 创建人
///
/// # 返回
/// 返回尚未持久化的合同与修订。
///
/// # 错误
/// 编号/快照为空、超长或有效期倒挂。
pub fn plan_upload_archive(
    req: UploadContractRequest,
    file_asset_id: FileAssetId,
    settlement_party_id: PartyId,
    actor_id: impl Into<String>,
) -> Result<PlannedContractArchive> {
    let actor_id = actor_id.into();
    let contract = new_contract(req.contract_no, req.customer_id, settlement_party_id.clone(), actor_id)?;
    let revision = build_revision(
        &contract,
        file_asset_id,
        ArchiveSource::ContractCenter,
        settlement_party_id,
        ArchiveSnapshotFields {
            customer_name: req.customer_name,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        },
        1,
    )?;
    Ok(PlannedContractArchive { contract, revision })
}

/// 由追加版本请求构造下一不可变修订。
///
/// # 参数
/// * `contract` - 所属合同
/// * `req` - 追加版本请求
/// * `current_revision_no` - 当前历史最大修订序号
///
/// # 返回
/// 返回携带新修订的规划结果（合同克隆自入参）。
///
/// # 错误
/// 序号溢出、快照非法或有效期倒挂时返回错误。
pub(crate) fn plan_next_revision(
    contract: &Contract,
    req: ArchiveContractRevisionRequest,
    current_revision_no: u32,
) -> Result<PlannedContractArchive> {
    let next_no = ContractRevision::next_revision_no(current_revision_no)?;
    let revision = build_revision(
        contract,
        req.contract_pdf_file_id,
        req.archive_source.unwrap_or(ArchiveSource::ContractCenter),
        contract.settlement_party_id.clone(),
        ArchiveSnapshotFields {
            customer_name: req.customer_name,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        },
        next_no,
    )?;
    Ok(PlannedContractArchive { contract: contract.clone(), revision })
}

/// 将客户事实映射为归档资格；缺失或停用保持原错误文案。
///
/// # 参数
/// * `customer` - 客户最小事实；`None` 表示不存在
///
/// # 返回
/// 返回可归档的客户事实。
///
/// # 错误
/// 缺失时返回 `NotFound`，停用时返回 `BusinessLogicError`。
pub(crate) fn customer_eligibility(
    customer: Option<crate::ports::CustomerAccountFact>,
) -> Result<crate::ports::CustomerAccountFact> {
    let customer = customer.ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
    if !customer.is_active {
        return Err(Error::BusinessLogicError("客户已停用，禁止归档新合同".to_string()));
    }
    Ok(customer)
}

/// 将实体版本匹配结果映射为稳定 409 语义.
///
/// # 参数
/// * `matched` - `Contract::matches_version(expected)` 的结果
///
/// # 返回
/// 匹配时返回 `Ok(())`。
///
/// # 错误
/// 不匹配时返回 `ConflictError`（HTTP 409）。
///
/// # 约束
/// 不比较版本号；调用方必须先调用实体 `matches_version`。
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
        let planned = plan_next_revision(&contract, req, 1).unwrap();
        assert_eq!(planned.revision.revision.revision_no, 2);
        assert!(
            plan_next_revision(
                &contract,
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
