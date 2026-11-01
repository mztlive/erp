//! 精确采购审批提交所关联供应商的最新登记评估；不改写冻结资料。

use erp_core::ids::{PurchaseOrderId, SupplierAccountId};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_supplier::entity::supplier::SupplierRatingRevision;
use erp_supplier::repository::SupplierExt;
use erp_workflow::service::approval::execution::runtime_service::ApprovalMaterialsView;
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::Serialize;

use crate::{Error, Result};

/// 最新登记评估与首次合作评分；空字段表示未登记，不表示零分。
#[derive(Debug, Serialize)]
pub struct ApprovalSupplierRating {
    pub rating_label: Option<String>,
    pub initial_score: Option<u8>,
    pub current_score: Option<u8>,
    pub revision_no: Option<u32>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

impl ApprovalSupplierRating {
    /// 仅从同一供应商历史中提取最新登记版本和首次合作评分。
    fn from_ratings(supplier_id: &SupplierAccountId, ratings: &[SupplierRatingRevision]) -> Result<Self> {
        if ratings.iter().any(|rating| &rating.supplier_id != supplier_id) {
            return Err(Error::ConflictError("供应商评估与采购提交不一致".into()));
        }
        let latest = ratings.iter().max_by_key(|rating| rating.revision.revision_no);
        let initial = ratings.iter().find(|rating| rating.revision.revision_no == 1);
        Ok(Self {
            rating_label: latest.map(|rating| rating.rating.label().to_owned()),
            initial_score: initial.and_then(|rating| rating.initial_score),
            current_score: latest.map(|rating| rating.current_score),
            revision_no: latest.map(|rating| rating.revision.revision_no),
            valid_from: latest.map(|rating| rating.valid_from.to_string()),
            valid_to: latest.and_then(|rating| rating.valid_to.map(|date| date.to_string())),
        })
    }
}

/// 实例授权后按精确采购提交读取供应商评估，不开放供应商目录。
/// # 参数
/// 数据库及已通过实例读取授权的提交资料。
/// # 返回
/// 采购审批返回最新登记评估，其他单据返回空；无评估返回字段为空的对象。
/// # 错误
/// 精确提交不存在、根单据或版本不符、供应商关系不符以及仓储失败时拒绝。
pub(crate) async fn supplier_rating(
    db: &Database,
    materials: &ApprovalMaterialsView,
) -> Result<Option<ApprovalSupplierRating>> {
    if materials.document_type != "purchase_order" {
        return Ok(None);
    }
    let submission = db
        .purchase_order_submissions()
        .find_by_order_and_submission_no(
            &PurchaseOrderId::new(&materials.document_id),
            &format!("SUB-{:06}", materials.subject_version),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::ConflictError("本次审批对应的采购提交不存在，无法核对供应商评估".into()))?;
    validate_source(materials, submission.purchase_order_id.as_ref(), submission.formal_sequence())?;
    let ratings =
        db.supplier().list_ratings_latest_first(&submission.supplier_id, &mut NoTransaction).await?;
    ApprovalSupplierRating::from_ratings(&submission.supplier_id, &ratings).map(Some)
}

/// 在读取供应商评价前证明精确采购提交属于当前审批。
fn validate_source(materials: &ApprovalMaterialsView, order_id: &str, version: Option<u32>) -> Result<()> {
    if materials.document_type != "purchase_order"
        || materials.document_id != order_id
        || materials.display.root_document_id != order_id
        || version != Some(materials.subject_version)
    {
        return Err(Error::ConflictError("供应商评估来源与本次采购审批不一致".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SupplierRatingRevisionId;
    use erp_supplier::entity::supplier::{SupplierRating, SupplierRatingRevisionData};
    use erp_workflow::entity::approval_integration::display_snapshot::ApprovalDisplaySnapshot;

    use super::*;

    fn rating(revision_no: u32, score: u8) -> SupplierRatingRevision {
        SupplierRatingRevision::new(
            SupplierRatingRevisionId::new(format!("rating-{revision_no}")),
            SupplierRatingRevisionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                revision_no,
                initial_score: (revision_no == 1).then_some(0),
                rating: SupplierRating::B,
                current_score: score,
                valid_from: "2026-10-01".parse().unwrap(),
                valid_to: None,
                change_reason: "定期评估".into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn latest_rating_keeps_original_score_and_zero_is_a_score() {
        let view = ApprovalSupplierRating::from_ratings(
            &SupplierAccountId::new("supplier-1"),
            &[rating(1, 80), rating(3, 0), rating(2, 90)],
        )
        .unwrap();
        assert_eq!(view.rating_label.as_deref(), Some("B 级"));
        assert_eq!(view.initial_score, Some(0));
        assert_eq!(view.current_score, Some(0));
        assert_eq!(view.revision_no, Some(3));
        assert_eq!(view.valid_from.as_deref(), Some("2026-10-01"));
        assert!(view.valid_to.is_none());
    }

    #[test]
    fn missing_evaluation_and_missing_first_revision_remain_unknown() {
        let id = SupplierAccountId::new("supplier-1");
        let empty = ApprovalSupplierRating::from_ratings(&id, &[]).unwrap();
        assert!(empty.rating_label.is_none());
        assert!(empty.current_score.is_none());
        assert!(empty.initial_score.is_none());
        let later = ApprovalSupplierRating::from_ratings(&id, &[rating(2, 90)]).unwrap();
        assert_eq!(later.current_score, Some(90));
        assert!(later.initial_score.is_none());
        assert!(matches!(
            ApprovalSupplierRating::from_ratings(&SupplierAccountId::new("other"), &[rating(1, 80)]),
            Err(Error::ConflictError(_))
        ));
    }

    #[test]
    fn supplier_lookup_requires_exact_purchase_root_and_version() {
        let mut materials = ApprovalMaterialsView {
            document_no: "PO-1".into(),
            document_type: "purchase_order".into(),
            document_id: "purchase-1".into(),
            subject_version: 2,
            display: ApprovalDisplaySnapshot::new("purchase-1".into()),
            attachments: vec![],
        };
        assert!(validate_source(&materials, "purchase-1", Some(2)).is_ok());
        for (id, version) in [("other", Some(2)), ("purchase-1", Some(1)), ("purchase-1", None)] {
            assert!(matches!(validate_source(&materials, id, version), Err(Error::ConflictError(_))));
        }
        materials.display.root_document_id = "other".into();
        assert!(validate_source(&materials, "purchase-1", Some(2)).is_err());
        materials.display.root_document_id = "purchase-1".into();
        materials.document_type = "sales_order".into();
        assert!(validate_source(&materials, "purchase-1", Some(2)).is_err());
    }
}
