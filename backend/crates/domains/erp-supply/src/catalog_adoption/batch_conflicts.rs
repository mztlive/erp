//! 同一来源商品的规格必须共用公司商品归属，冲突不得拆行绕过。

use std::collections::{BTreeSet, HashMap, HashSet};

use super::{BatchItemStatus, CatalogAdoptionBatchItem};
use crate::{Error, Result};

/// 将同一来源商品的跨规格映射冲突标记到该商品全部待处理规格。
/// # 参数
/// `rows` 为已完成本轮自动匹配的完整批次，不得用当前页替代。
/// # 返回
/// 有冲突的商品全部取消提交资格；无冲突商品、排除项和已执行结果保持原值。
/// # 错误
/// 完整集合包含重复来源 SKU 时拒绝，禁止以重复行建立不同公司归属。
pub fn mark_product_mapping_conflicts(rows: &mut [CatalogAdoptionBatchItem]) -> Result<()> {
    let mut groups = HashMap::<(String, String), Vec<usize>>::new();
    let mut seen = HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        if !row.editable() || row.status == BatchItemStatus::Excluded {
            continue;
        }
        if !seen.insert(&row.source.base.id) {
            return Err(Error::ValidationError("批次来源规格重复，请重新建立选择范围".into()));
        }
        groups
            .entry((row.source.connection_id.clone(), row.source.product_id.clone()))
            .or_default()
            .push(index);
    }
    for indices in groups.values() {
        let Some(reason) = conflict_reason(rows, indices) else { continue };
        for index in indices {
            let row = &mut rows[*index];
            row.status = BatchItemStatus::Draft;
            row.frozen_row = None;
            row.preview_version = None;
            row.warnings_confirmed = false;
            if !row.messages.iter().any(|message| message == reason) {
                row.messages.push(reason.into());
            }
        }
    }
    Ok(())
}

fn conflict_reason(rows: &[CatalogAdoptionBatchItem], indices: &[usize]) -> Option<&'static str> {
    let mut products = BTreeSet::new();
    let mut new_product_classes = BTreeSet::new();
    for index in indices {
        let row = &rows[*index];
        let Some(resolution) = &row.resolution else { continue };
        if let Some(product) = &resolution.product_id {
            products.insert(product.as_str());
        }
        // 已有商品和追加规格沿用实际公司分类，用户尚未生效的映射不能制造冲突。
        if resolution.kind == "CREATE_PRODUCT" {
            new_product_classes.insert((row.mapping.category_id.as_str(), row.mapping.product_kind.as_str()));
        }
    }
    if products.len() > 1 {
        return Some("同一来源商品的规格匹配到不同公司商品，请核对来源身份；本商品全部规格暂不可入库");
    }
    (new_product_classes.len() > 1)
        .then_some("同一来源商品的公司分类或商品类型映射不一致，请统一调整；本商品全部规格暂不可入库")
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::catalog_adoption::{AdoptionMapping, BatchResolution, BatchSalesValues};
    use crate::entity::supplier_catalog::SupplierCatalogItem;
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::offer::{Offer, OfferDetails};

    fn row(product: &str, specification: &str, category: &str) -> CatalogAdoptionBatchItem {
        let source = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "sync",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some(product.into()), spec_id: specification.into() },
                    name: "大米".into(),
                    specification: specification.into(),
                    unit: Some("袋".into()),
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap();
        CatalogAdoptionBatchItem {
            base: BaseModel::fake(),
            batch_id: "batch".into(),
            source,
            previous_source: None,
            initially_adopted: false,
            mapping: AdoptionMapping {
                category_id: category.into(),
                unit_id: "bag".into(),
                product_kind: "PHYSICAL".into(),
                company_name: None,
                company_description: None,
            },
            terms: None,
            sales_prices: BatchSalesValues::default(),
            delivery_template_id: None,
            field_origins: Default::default(),
            status: BatchItemStatus::Ready,
            messages: vec![],
            warnings: vec![],
            warnings_confirmed: true,
            resolution: Some(BatchResolution {
                kind: "CREATE_PRODUCT".into(),
                expected_match: "reviewed".into(),
                sku_no: None,
                product_id: None,
                reason: String::new(),
                group_anchor: None,
            }),
            frozen_row: None,
            result: None,
            operation_id: None,
            submitted_by: None,
            processing_started_at: None,
            preview_version: Some(2),
        }
    }

    #[test]
    fn conflicting_category_blocks_entire_source_product_but_preserves_other_products() {
        let mut rows = vec![row("rice", "5kg", "food"), row("rice", "10kg", "gift"), row("oil", "5L", "oil")];
        mark_product_mapping_conflicts(&mut rows).unwrap();
        for row in &rows[..2] {
            assert_eq!(row.status, BatchItemStatus::Draft);
            assert!(row.preview_version.is_none());
            assert!(!row.warnings_confirmed);
            assert!(row.messages[0].contains("公司分类或商品类型映射不一致"));
        }
        assert_eq!(rows[2].status, BatchItemStatus::Ready);
        assert_eq!(rows[2].preview_version, Some(2));
        mark_product_mapping_conflicts(&mut rows).unwrap();
        assert_eq!(rows[0].messages.len(), 1);
    }

    #[test]
    fn source_product_cannot_link_specifications_to_two_company_products() {
        let mut rows = vec![row("rice", "5kg", "ignored"), row("rice", "10kg", "different")];
        for (row, target) in rows.iter_mut().zip(["company-a", "company-b"]) {
            let resolution = row.resolution.as_mut().unwrap();
            resolution.kind = "EXISTING".into();
            resolution.product_id = Some(target.into());
        }
        mark_product_mapping_conflicts(&mut rows).unwrap();
        assert!(rows.iter().all(|row| row.status == BatchItemStatus::Draft));
        assert!(rows[0].messages[0].contains("不同公司商品"));
    }

    #[test]
    fn existing_product_uses_retained_classification_instead_of_unapplied_draft_mappings() {
        let mut rows = vec![row("rice", "5kg", "ignored"), row("rice", "10kg", "different")];
        for (row, kind) in rows.iter_mut().zip(["EXISTING", "CREATE_SKU"]) {
            let resolution = row.resolution.as_mut().unwrap();
            resolution.kind = kind.into();
            resolution.product_id = Some("company".into());
        }
        mark_product_mapping_conflicts(&mut rows).unwrap();
        assert!(rows.iter().all(|row| row.status == BatchItemStatus::Ready));
        assert!(rows.iter().all(|row| row.messages.is_empty()));
    }

    #[test]
    fn excluded_and_completed_rows_are_preserved_and_cannot_create_mapping_conflicts() {
        let mut rows =
            vec![row("rice", "5kg", "food"), row("rice", "10kg", "other"), row("rice", "20kg", "other")];
        rows[1].status = BatchItemStatus::Excluded;
        rows[2].status = BatchItemStatus::Succeeded;
        rows[2].operation_id = Some("completed".into());
        mark_product_mapping_conflicts(&mut rows).unwrap();
        assert_eq!(rows[0].status, BatchItemStatus::Ready);
        assert_eq!(rows[1].status, BatchItemStatus::Excluded);
        assert_eq!(rows[2].status, BatchItemStatus::Succeeded);
        assert_eq!(rows[2].operation_id.as_deref(), Some("completed"));
    }

    #[test]
    fn type_conflict_and_duplicate_source_membership_are_rejected() {
        let mut rows = vec![row("rice", "5kg", "food"), row("rice", "10kg", "food")];
        rows[1].mapping.product_kind = "VIRTUAL".into();
        mark_product_mapping_conflicts(&mut rows).unwrap();
        assert!(rows.iter().all(|row| row.status == BatchItemStatus::Draft));
        let first = row("rice", "5kg", "food");
        let mut duplicate = vec![first.clone(), first];
        assert!(mark_product_mapping_conflicts(&mut duplicate).is_err());
    }
}
