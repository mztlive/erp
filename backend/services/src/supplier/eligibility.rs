//! 供应商能力与资质的下游统一准入门禁。
//!
//! 供给启用、采购确认和采购建单必须调用本模块；只维护资质数据而不接入这些
//! 动作不构成业务约束。

use database::{FileAssetExt, NoTransaction, SupplierExt};
use entities::{
    common::time::{BusinessDate, Instant},
    ids::{SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId},
    supplier::{CapabilityStatus, SupplierCapabilityRevision, SupplierQualification},
};
use mongodb::{bson::doc, Database};

use crate::errors::{Error, Result};

/// 校验指定供应商能力版本在业务日仍为当前启用版本，且至少存在一份适用有效资质。
///
/// # Errors
/// 供应商/能力停用、版本过期或变化、无有效资质、资质附件不可用时返回业务错误。
pub(crate) async fn ensure_capability_qualified(
    db: &Database,
    supplier_id: &SupplierAccountId,
    capability_revision_id: &SupplierCapabilityRevisionId,
    on_date: BusinessDate,
) -> Result<()> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    if !supplier.is_active() {
        return Err(Error::BusinessLogicError(
            "供应商已停用，不能用于供给或采购".to_string(),
        ));
    }
    let revision = load_capability_revision(db, capability_revision_id).await?;
    let capability = db
        .supplier_capabilities()
        .find_by_supplier_and_code(supplier_id, revision.capability_code, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商能力不存在".to_string()))?;
    let current = capability.stable.current_revision_id.as_deref() == Some(capability_revision_id.as_ref());
    let in_window = revision.valid_from <= on_date && revision.valid_to.is_none_or(|end| on_date <= end);
    if revision.supplier_id != *supplier_id
        || revision.status != CapabilityStatus::Active
        || !capability.is_active()
        || !current
        || !in_window
    {
        return Err(Error::BusinessLogicError(
            "供应商能力已停用、过期或版本已变化".to_string(),
        ));
    }
    ensure_linked_qualification(db, &capability.base.id, on_date).await
}

/// 加载能力修订。
async fn load_capability_revision(
    db: &Database,
    revision_id: &SupplierCapabilityRevisionId,
) -> Result<SupplierCapabilityRevision> {
    db.supplier_capability_revisions()
        .find_by_id(revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商能力版本不存在".to_string()))
}

/// 校验能力至少关联一份业务日有效且附件可用的资质。
async fn ensure_linked_qualification(
    db: &Database,
    capability_id: &str,
    on_date: BusinessDate,
) -> Result<()> {
    let links = db
        .supplier_qualification_capabilities()
        .list_by_capability_id(&SupplierCapabilityId::new(capability_id), &mut NoTransaction)
        .await?;
    let ids: Vec<String> = links
        .iter()
        .map(|link| link.qualification_id.to_string())
        .collect();
    let qualifications: Vec<SupplierQualification> = db
        .supplier_qualifications()
        .find_many(doc! { "id": { "$in": ids } }, &mut NoTransaction)
        .await?;
    for qualification in qualifications {
        if qualification.is_valid()
            && qualification.valid_from <= on_date
            && qualification.valid_to.is_none_or(|end| on_date <= end)
            && attachment_usable(db, qualification.attachment_id.as_ref()).await?
        {
            return Ok(());
        }
    }
    Err(Error::BusinessLogicError(
        "供应商该项能力没有适用且有效的资质，不能用于供给或采购".to_string(),
    ))
}

/// 无附件的结构化资质可参与门禁；有附件时必须通过扫描且仍在保留期。
async fn attachment_usable(
    db: &Database,
    attachment_id: Option<&entities::ids::FileAssetId>,
) -> Result<bool> {
    let Some(attachment_id) = attachment_id else {
        return Ok(true);
    };
    let asset = db
        .file_assets()
        .find_by_id(attachment_id, &mut NoTransaction)
        .await?;
    Ok(asset.is_some_and(|asset| asset.is_usable_for_business(Instant::now())))
}
