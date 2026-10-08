//! 已授权采购来源的销售修订材料；不授予普通销售、合同或文件目录访问。

use std::collections::BTreeSet;

use erp_contract::{ContractExt, ContractRevision};
use erp_core::ids::{ContractRevisionId, CustomerAccountId, FileAssetId, SalesOrderId, SalesOrderRevisionId};
use erp_sales::entity::sales_order::{BusinessType, SalesOrderRevision, SalesOrderSubmission};
use erp_sales::entity::sales_review::SalesChangeSubmission;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_support::repository::FileAssetExt;
use erp_support::repository::prelude::*;
use erp_support::{FileAsset, SecurityScanStatus};
use erp_workflow::entity::approval_integration::{ApprovalMaterialFile, ApprovalSubjectSnapshot};
use erp_workflow::repository::prelude::*;
use erp_workflow::{ApprovalIntegrationExt, DocumentType};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub mod repository;

/// 限定销售正式修订的材料；文件版本及内容标识仅供服务器授权重验。
pub struct RevisionMaterials {
    pub files: Vec<ApprovalMaterialFile>,
    pub contract: Option<ContractRevision>,
}

/// 销售冻结提交明确保存的合同关系，不包含合同当前修订指针。
struct ContractSource<'a> {
    sales_order_id: &'a SalesOrderId,
    customer_id: &'a CustomerAccountId,
    revision_id: Option<&'a ContractRevisionId>,
    contract_no: Option<&'a str>,
}

/// 已通过销售审批的来源身份，不接受当前销售草稿替代。
#[derive(Clone)]
struct SalesApprovalSource {
    document_type: DocumentType,
    document_id: String,
    submission_no: u32,
    customer_id: CustomerAccountId,
    base_revision_id: Option<SalesOrderRevisionId>,
}

/// 读取销售正式修订锁定的合同 PDF 与原销售审批冻结的附件。
///
/// # 参数
/// * `db` - 销售与文件事实所在数据库。
/// * `revision` - 已授权采购来源读取阶段锁定的销售修订。
/// * `executor` - 同一读取阶段的执行器。
/// # 返回
/// 返回精确合同修订和已验证的冻结材料，旧审批未保存的附件保持缺失。
/// # 错误
/// 来源提交不一致、文件变化、删除或安全隔离时拒绝，不补读当前销售附件。
/// 仓储读取失败会返回对应错误。
pub async fn revision_materials(
    db: &Database,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> Result<RevisionMaterials> {
    let source = revision_source(db, revision, executor).await?;
    let contract = revision_contract(db, revision, source.as_ref(), executor).await?;
    let mut files = revision_frozen_materials(db, revision, source.as_ref(), executor).await?;
    if let Some(contract) = &contract {
        let id = contract.contract_pdf_file_id.as_ref();
        if files
            .iter()
            .any(|file| file.file_asset_id.as_ref() == id && file.content_type != "application/pdf")
        {
            return Err(Error::ConflictError("来源销售锁定合同正文不是 PDF".into()));
        }
        if !files.iter().any(|file| file.file_asset_id.as_ref() == id) {
            let asset = db
                .file_assets()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("来源销售合同 PDF 不存在".into()))?;
            require_contract_pdf(&asset)?;
            files.push(material_from_asset(&asset));
        }
    }
    if files.len() > 100 {
        return Err(Error::ConflictError("销售合同与凭证超过100个文件".into()));
    }
    validate_materials(db, &files, executor).await?;
    Ok(RevisionMaterials { files, contract })
}

/// 重新核对冻结文件版本、内容标识及当前治理状态。
///
/// # 参数
/// * `db` - 文件事实所在数据库。
/// * `files` - 已经由销售或采购来源链证明的精确允许清单。
/// * `executor` - 调用方执行器。
/// # 返回
/// 全部文件仍匹配冻结内容且可供读取时成功。
/// # 错误
/// 缺失、软删除、文件变化、销毁或隔离时拒绝整个材料包。仓储读取失败会返回对应错误。
pub async fn validate_materials(
    db: &Database,
    files: &[ApprovalMaterialFile],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = files.iter().map(|file| file.file_asset_id.clone()).collect::<Vec<_>>();
    let current = db.file_assets().find_by_ids(&ids, executor).await?;
    if current.len() != files.len() {
        return Err(Error::ConflictError("来源销售材料不存在或已删除".into()));
    }
    for file in files {
        let asset = current
            .iter()
            .find(|asset| asset.base.id == file.file_asset_id.as_ref())
            .ok_or_else(|| Error::ConflictError("来源销售材料关联已变化".into()))?;
        validate_asset(file, asset)?;
    }
    Ok(())
}

/// 从真实文件实体捕获同版本审批材料；调用方仍须证明其业务来源。
///
/// # 参数
/// `asset` 为当前事务读取的文件事实。
/// # 返回
/// 返回包含资产版本和内容标识的冻结引用。
/// # 错误
/// 无；读取前必须通过 `validate_materials` 校验。
pub fn material_from_asset(asset: &FileAsset) -> ApprovalMaterialFile {
    ApprovalMaterialFile {
        file_asset_id: FileAssetId::new(&asset.base.id),
        file_name: asset.file_name.clone(),
        content_type: asset.content_type.clone(),
        byte_size: asset.byte_size,
        asset_version: asset.base.version,
        content_hmac: asset.content_hmac.as_str().to_string(),
    }
}

/// 合同正文必须为归档 PDF，不能将错挂的活动内容当作合同转授。
/// # 参数
/// `asset` 为精确合同修订指向的真实文件。
/// # 返回
/// PDF 类型时成功。
/// # 错误
/// 类型与合同正文约定不符时拒绝。
pub fn require_contract_pdf(asset: &FileAsset) -> Result<()> {
    if asset.content_type != "application/pdf" {
        return Err(Error::ConflictError("锁定合同正文不是 PDF，请核对归档文件".into()));
    }
    Ok(())
}

/// 正式修订只读取其不可变合同指针，不读取合同当前修订。
async fn revision_contract(
    db: &Database,
    revision: &SalesOrderRevision,
    source: Option<&SalesApprovalSource>,
    executor: &mut dyn Executor,
) -> Result<Option<ContractRevision>> {
    let Some(id) = &revision.contract_revision_id else { return Ok(None) };
    let contract = db
        .contract_revisions()
        .find_by_id(id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("来源销售锁定的合同版本不存在".into()))?;
    let identity = db
        .contracts()
        .find_by_id(contract.contract_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("来源销售锁定的合同不存在".into()))?;
    let customer_id = if let Some(source) = source {
        source.customer_id.clone()
    } else {
        let order = db
            .sales_orders()
            .find_by_id(revision.sales_order_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("来源销售单不存在".into()))?;
        order.customer_id
    };
    contract
        .ensure_customer_reference(
            &identity,
            &customer_id,
            revision.contract_snapshot.as_ref().map(|value| value.contract_no.as_str()),
        )
        .map_err(|_| Error::ConflictError("来源销售与合同版本不匹配".into()))?;
    if revision.customer_snapshot.customer_name != contract.customer_snapshot.customer_name {
        return Err(Error::ConflictError("来源销售与合同版本不匹配".into()));
    }
    Ok(Some(contract))
}

/// 按本次销售提交锁定的合同版本证明客户与合同关系。
///
/// # 参数
/// * `db` - 合同与销售事实所在数据库。
/// * `submission` - 本次审批精确版本的不可变销售提交。
/// * `executor` - 调用方执行器。
/// # 返回
/// 返回锁定的合同版本；无合同开单返回 `None`。
/// # 错误
/// 版本缺失或客户、合同身份、合同编号不匹配时拒绝；销售结算主体允许单独选择。
/// 仓储读取失败会返回对应错误。
pub async fn submission_contract(
    db: &Database,
    submission: &SalesOrderSubmission,
    executor: &mut dyn Executor,
) -> Result<Option<ContractRevision>> {
    locked_contract(
        db,
        ContractSource {
            sales_order_id: &submission.sales_order_id,
            customer_id: &submission.customer_id,
            revision_id: submission.contract_revision_id.as_ref(),
            contract_no: submission.contract_snapshot.as_ref().map(|value| value.contract_no.as_str()),
        },
        executor,
    )
    .await
}

/// 读取本次销售变更提交锁定的合同正文关系。
/// # 参数
/// * `db` - 合同与销售事实所在数据库。
/// * `submission` - 精确变更提交。
/// * `executor` - 原审批启动事务。
/// # 返回
/// 返回经过真实销售单和客户关系证明的合同修订。没有锁定合同修订时返回 `None`。
/// # 错误
/// 关系缺失或不匹配时失败关闭，不补录当前合同。仓储读取失败会返回对应错误。
pub async fn change_submission_contract(
    db: &Database,
    submission: &SalesChangeSubmission,
    executor: &mut dyn Executor,
) -> Result<Option<ContractRevision>> {
    locked_contract(
        db,
        ContractSource {
            sales_order_id: &submission.sales_order_id,
            customer_id: &submission.customer_id,
            revision_id: submission.contract_revision_id.as_ref(),
            contract_no: submission.contract_snapshot.as_ref().map(|value| value.contract_no.as_str()),
        },
        executor,
    )
    .await
}

/// 变更审批沿准确基准修订继承原销售冻结凭证，不继承旧合同正文。
/// # 参数
/// * `db` - 销售修订与文件事实所在数据库。
/// * `submission` - 本次变更审批的精确提交。
/// * `executor` - 原事务执行器。
/// # 返回
/// 仍可读取的原成交冻结材料；旧清单为空时保持为空。
/// # 错误
/// 跨销售单、版本链错误或历史文件变化时拒绝材料。仓储读取失败会返回对应错误。
pub async fn change_evidence(
    db: &Database,
    submission: &SalesChangeSubmission,
    executor: &mut dyn Executor,
) -> Result<Vec<ApprovalMaterialFile>> {
    let base = db
        .sales_order_revisions()
        .find_by_id(submission.base_revision_id.as_ref(), executor)
        .await?
        .filter(|base| base.sales_order_id == submission.sales_order_id)
        .ok_or_else(|| Error::ConflictError("销售变更基准版本与销售单不匹配".into()))?;
    let source = revision_source(db, &base, executor).await?;
    let mut files = revision_frozen_materials(db, &base, source.as_ref(), executor).await?;
    remove_previous_contract(db, &base, &mut files, executor).await?;
    validate_materials(db, &files, executor).await?;
    Ok(files)
}

/// 唯一不可变合同修订必须同时匹配真实销售单、客户和合同编号。
async fn locked_contract(
    db: &Database,
    source: ContractSource<'_>,
    executor: &mut dyn Executor,
) -> Result<Option<ContractRevision>> {
    let Some(id) = source.revision_id else { return Ok(None) };
    let revision = db
        .contract_revisions()
        .find_by_id(id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售提交锁定的合同版本不存在".into()))?;
    let contract = db
        .contracts()
        .find_by_id(revision.contract_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售提交关联合同不存在".into()))?;
    let order = db
        .sales_orders()
        .find_by_id(source.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售提交对应销售单不存在".into()))?;
    revision
        .ensure_customer_reference(&contract, source.customer_id, source.contract_no)
        .map_err(|_| Error::ConflictError("销售提交与锁定合同的客户或合同关系不匹配".into()))?;
    if order.contract_id.as_ref() != Some(&revision.contract_id) {
        return Err(Error::ConflictError("销售提交与锁定合同的客户或合同关系不匹配".into()));
    }
    Ok(Some(revision))
}

/// 当前文件必须匹配冻结引用且未被治理禁止读取。
fn validate_asset(file: &ApprovalMaterialFile, asset: &FileAsset) -> Result<()> {
    file.validate()?;
    if !file.matches_current(&material_from_asset(asset)) {
        return Err(Error::ConflictError("来源销售材料已变化，请重新核对".into()));
    }
    if asset.destroyed_at.is_some()
        || matches!(
            asset.security_scan_status,
            SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined
        )
    {
        return Err(Error::Forbidden("来源销售材料当前不可读取".into()));
    }
    Ok(())
}

/// 正式修订通过持久化内容身份关联精确销售提交与冻结审批材料。
async fn revision_frozen_materials(
    db: &Database,
    revision: &SalesOrderRevision,
    source: Option<&SalesApprovalSource>,
    executor: &mut dyn Executor,
) -> Result<Vec<ApprovalMaterialFile>> {
    let mut source = source.cloned();
    let mut child = revision.clone();
    let mut visited = BTreeSet::from([child.base.id.clone()]);
    let mut files = Vec::new();
    let mut inherited = false;
    while let Some(current) = source {
        let mut captured = source_materials(db, &current, executor).await?;
        if inherited {
            remove_previous_contract(db, &child, &mut captured, executor).await?;
        }
        files.extend(captured);
        let Some(base_id) = &current.base_revision_id else { break };
        let base = db
            .sales_order_revisions()
            .find_by_id(base_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("销售变更原版本不存在".into()))?;
        validate_inheritance(&child, &base, &mut visited)?;
        source = revision_source(db, &base, executor).await?;
        child = base;
        inherited = true;
    }
    files.sort_by(|left, right| left.file_asset_id.as_ref().cmp(right.file_asset_id.as_ref()));
    files.dedup();
    if files.len() > 100 {
        return Err(Error::ConflictError("销售继承材料超过100个文件".into()));
    }
    Ok(files)
}

/// 每个继承节点只读取其原审批冻结白名单，空旧清单保持为空。
async fn source_materials(
    db: &Database,
    source: &SalesApprovalSource,
    executor: &mut dyn Executor,
) -> Result<Vec<ApprovalMaterialFile>> {
    let snapshots = db
        .approval_subject_snapshots()
        .list_by_business_objects(&[(source.document_type, source.document_id.clone())], executor)
        .await?;
    selected_materials(&snapshots, source.document_type, &source.document_id, source.submission_no)
}

/// 上一版本的合同正文不作为本版凭证继承；本版合同仍由精确不可变指针独立读取。
async fn remove_previous_contract(
    db: &Database,
    revision: &SalesOrderRevision,
    files: &mut Vec<ApprovalMaterialFile>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(id) = &revision.contract_revision_id else { return Ok(()) };
    let contract = db
        .contract_revisions()
        .find_by_id(id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售原版本锁定合同不存在".into()))?;
    files.retain(|file| file.file_asset_id != contract.contract_pdf_file_id);
    Ok(())
}

/// 继承关系必须指向同单更旧版本，且每个版本只访问一次。
fn validate_inheritance(
    child: &SalesOrderRevision,
    base: &SalesOrderRevision,
    visited: &mut BTreeSet<String>,
) -> Result<()> {
    if child.sales_order_id != base.sales_order_id
        || base.revision.revision_no >= child.revision.revision_no
        || visited.len() >= 100
        || !visited.insert(base.base.id.clone())
    {
        return Err(Error::ConflictError("销售凭证继承版本关系无效".into()));
    }
    Ok(())
}

/// 导入版本没有销售审批材料；审批版本的提交身份必须与修订精确匹配。
async fn revision_source(
    db: &Database,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> Result<Option<SalesApprovalSource>> {
    let Some(id) = revision.content_hash.strip_prefix("sub:") else { return Ok(None) };
    if let Some(submission) = db.sales_order_submissions().find_by_id(id, executor).await? {
        if submission.sales_order_id != revision.sales_order_id
            || submission.contract_revision_id != revision.contract_revision_id
            || submission.gross_amount != revision.gross_amount
        {
            return Err(Error::ConflictError("销售版本与原审批提交不一致".into()));
        }
        let document_type = match submission.business_type {
            BusinessType::Voucher => DocumentType::VoucherSalesOrder,
            BusinessType::GoodsService => DocumentType::SalesOrder,
        };
        return Ok(Some(SalesApprovalSource {
            document_type,
            document_id: submission.sales_order_id.to_string(),
            submission_no: submission.submission_no,
            customer_id: submission.customer_id,
            base_revision_id: None,
        }));
    }
    let submission = db
        .sales_change_submissions()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售版本原审批提交不存在".into()))?;
    if submission.sales_order_id != revision.sales_order_id
        || submission.contract_revision_id != revision.contract_revision_id
        || submission.gross_amount != revision.gross_amount
    {
        return Err(Error::ConflictError("销售版本与原变更审批提交不一致".into()));
    }
    Ok(Some(SalesApprovalSource {
        document_type: DocumentType::SalesChangeOrder,
        document_id: submission.sales_change_order_id.to_string(),
        submission_no: submission.submission_no,
        customer_id: submission.customer_id,
        base_revision_id: Some(submission.base_revision_id),
    }))
}

/// 只挑精确类型、对象和提交版本，旧快照材料为空时保持为空。
fn selected_materials(
    snapshots: &[ApprovalSubjectSnapshot],
    kind: DocumentType,
    id: &str,
    version: u32,
) -> Result<Vec<ApprovalMaterialFile>> {
    let mut selected = snapshots
        .iter()
        .filter(|snapshot| snapshot.ensure_matches_runtime_subject(kind, id, version).is_ok());
    let Some(snapshot) = selected.next() else { return Ok(Vec::new()) };
    if selected.next().is_some() {
        return Err(Error::ConflictError("销售提交存在重复审批材料快照".into()));
    }
    Ok(snapshot.material_files.clone())
}

#[cfg(test)]
mod tests {

    use bpm::ApprovalProcessInstanceId;
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{ApprovalSubjectSnapshotId, SalesOrderId};
    use erp_sales::entity::sales_order::{HeaderSnapshotData, RevisionSource, SalesOrderRevisionData};
    use erp_support::{ContentHmac, FileAssetData, RetentionClass, SensitivityClass};
    use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;

    use super::*;

    fn asset() -> FileAsset {
        let mut asset = FileAsset::new(
            FileAssetId::new("pdf-1"),
            FileAssetData {
                storage_object_key: "materials/pdf-1".into(),
                file_name: "合同.pdf".into(),
                content_type: "application/pdf".into(),
                byte_size: 12,
                content_hmac: ContentHmac::parse("a".repeat(64)).unwrap(),
                sensitivity_class: SensitivityClass::General,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
                created_by: "sales".into(),
            },
        )
        .unwrap();
        asset.base = BaseModel::fake();
        asset.base.id = "pdf-1".into();
        asset
    }

    fn snapshot(kind: DocumentType, id: &str, version: u32) -> ApprovalSubjectSnapshot {
        ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new(format!("snapshot-{version}")),
            ApprovalProcessInstanceId::new(format!("instance-{version}")),
            kind,
            id,
            version,
            ApprovalSubjectSnapshotPayload {
                document_no: "XS-1".into(),
                responsible_org_id: "org".into(),
                submitted_by: "sales".into(),
                submitted_at: Instant::from_unix_secs(1_800_000_000),
                counterparty: None,
                total_amount: Some("12".parse().unwrap()),
                total_quantity: Some("1".parse().unwrap()),
                line_count: 1,
            },
        )
        .unwrap()
    }

    fn revision(id: &str, order: &str, number: u32) -> SalesOrderRevision {
        SalesOrderRevision::new(
            SalesOrderRevisionId::new(id),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new(order),
                revision_no: number,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: format!("sub:{id}"),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".into(),
                    contract_no: None,
                    settlement_party_name: None,
                    payment_term_code: "NET30".into(),
                    payment_term_name: "月结30天".into(),
                    invoice_type: "普通发票".into(),
                    tax_point: "0".into(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: "12".parse().unwrap(),
                net_amount: "12".parse().unwrap(),
                tax_amount: "0".parse().unwrap(),
                effective_at: Instant::from_unix_secs(1_800_000_000),
                recorded_at: Instant::from_unix_secs(1_800_000_000),
            },
        )
        .unwrap()
    }

    #[test]
    fn evidence_inheritance_accepts_only_unvisited_older_revisions_of_the_same_sale() {
        let child = revision("new", "sales-1", 2);
        let base = revision("old", "sales-1", 1);
        let mut visited = BTreeSet::from([child.base.id.clone()]);
        validate_inheritance(&child, &base, &mut visited).unwrap();
        assert!(validate_inheritance(&child, &base, &mut visited).is_err());
        let mut fresh = BTreeSet::from([child.base.id.clone()]);
        assert!(validate_inheritance(&child, &revision("foreign", "sales-other", 1), &mut fresh).is_err());
        assert!(validate_inheritance(&child, &revision("future", "sales-1", 3), &mut fresh).is_err());
        assert!(validate_inheritance(&child, &child, &mut fresh).is_err());
    }

    #[test]
    fn only_exact_source_submission_supplies_materials_and_legacy_stays_empty() {
        let file = material_from_asset(&asset());
        let selected =
            snapshot(DocumentType::SalesOrder, "sales-1", 2).with_material_files(vec![file.clone()]).unwrap();
        let legacy = snapshot(DocumentType::SalesOrder, "sales-1", 1);
        assert_eq!(
            selected_materials(&[legacy.clone(), selected.clone()], DocumentType::SalesOrder, "sales-1", 2)
                .unwrap(),
            vec![file]
        );
        for (kind, id, version) in [
            (DocumentType::VoucherSalesOrder, "sales-1", 2),
            (DocumentType::SalesOrder, "sales-other", 2),
            (DocumentType::SalesOrder, "sales-1", 3),
        ] {
            assert!(
                selected_materials(std::slice::from_ref(&selected), kind, id, version).unwrap().is_empty()
            );
        }
        assert!(selected_materials(&[legacy], DocumentType::SalesOrder, "sales-1", 1).unwrap().is_empty());
        assert!(
            selected_materials(&[selected.clone(), selected], DocumentType::SalesOrder, "sales-1", 2)
                .is_err()
        );
    }

    #[test]
    fn changed_and_quarantined_assets_are_rejected() {
        let original = asset();
        let file = material_from_asset(&original);
        validate_asset(&file, &original).unwrap();
        let mut changed = original.clone();
        changed.base.version += 1;
        assert!(matches!(validate_asset(&file, &changed), Err(Error::ConflictError(_))));
        changed = original;
        changed.security_scan_status = SecurityScanStatus::Quarantined;
        assert!(matches!(validate_asset(&file, &changed), Err(Error::Forbidden(_))));
        require_contract_pdf(&asset()).unwrap();
        changed.content_type = "text/html".into();
        assert!(matches!(require_contract_pdf(&changed), Err(Error::ConflictError(_))));
    }
}
