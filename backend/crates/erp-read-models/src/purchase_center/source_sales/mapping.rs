//! 销售版本和采购引用的纯映射；缺失关系不得补零价或读取最新 SKU 参考价。

use std::collections::{BTreeMap, BTreeSet};

use erp_core::ids::SalesOrderRevisionId;
use erp_procurement::dto::purchase_order::PurchaseOrderLineView;
use erp_procurement::entity::purchase_order::PurchaseLineType;
use erp_sales::entity::sales_order::{LineType, SalesOrderGoodsServiceLineRevision, SalesOrderRevisionLine};
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;

use super::super::dto::{PurchaseSourceSalesLineView, PurchaseSourceSalesMaterialView};
use crate::{Error, Result};

/// 精确证明采购商品行身份及版本；多版本或缺失关系整体拒绝，物流费不产生销售价。
///
/// # 参数
/// * `purchase_lines` - 采购行。
/// * `sales_lines` - 被引用的销售版本公共行。
/// * `creation_revision` - 建单时的销售版本；没有商品行引用时采用它。
///
/// # 返回
/// 返回唯一销售版本身份；仅非商品行时返回 `creation_revision`。
///
/// # 错误
/// 商品行缺少或对不上销售行、行类型不是实物服务，或同时关联多个销售版本时返回 `BusinessLogicError`。
pub(super) fn source_revision_id(
    purchase_lines: &[PurchaseOrderLineView],
    sales_lines: &[SalesOrderRevisionLine],
    creation_revision: &SalesOrderRevisionId,
) -> Result<SalesOrderRevisionId> {
    let sales = sales_lines.iter().map(|line| (line.base.id.as_str(), line)).collect::<BTreeMap<_, _>>();
    let mut revisions = BTreeSet::new();
    for purchase in purchase_lines.iter().filter(|line| line.line_type == PurchaseLineType::ItemService) {
        let id = purchase.sales_order_revision_line_id.as_deref().ok_or_else(missing_relation)?;
        let line = sales.get(id).ok_or_else(missing_relation)?;
        if line.line_type != LineType::GoodsService
            || purchase.sales_order_line_id.as_deref() != Some(line.sales_order_line_id.as_ref())
        {
            return Err(missing_relation());
        }
        revisions.insert(line.sales_order_revision_id.to_string());
    }
    if revisions.len() > 1 {
        return Err(Error::BusinessLogicError("采购明细关联多个销售版本，请联系经办人核对".into()));
    }
    Ok(revisions
        .into_iter()
        .next()
        .map(SalesOrderRevisionId::new)
        .unwrap_or_else(|| creation_revision.clone()))
}

/// 版本公共行与实物服务子行按版本行身份一对一关联，保留成交价的精确十进制。
///
/// # 参数
/// * `lines` - 销售版本公共行。
/// * `goods` - 同一版本的实物服务子行。
///
/// # 返回
/// 返回实物服务行视图，成交价保持原十进制字符串。
///
/// # 错误
/// 实物服务公共行找不到对应子行时返回 `BusinessLogicError`。
pub(super) fn sales_line_views(
    lines: &[SalesOrderRevisionLine],
    goods: &[SalesOrderGoodsServiceLineRevision],
) -> Result<Vec<PurchaseSourceSalesLineView>> {
    let goods = goods.iter().map(|line| (line.revision_line_id.as_ref(), line)).collect::<BTreeMap<_, _>>();
    lines
        .iter()
        .filter(|line| line.line_type == LineType::GoodsService)
        .map(|line| {
            let goods = goods.get(line.base.id.as_str()).ok_or_else(missing_relation)?;
            Ok(PurchaseSourceSalesLineView {
                sales_order_revision_line_id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.to_string(),
                line_no: line.line_no,
                item_name: line.item_name_snapshot.clone(),
                specification: line.spec_snapshot.clone(),
                quantity: goods.quantity.to_string(),
                unit: line.unit_snapshot.clone().unwrap_or_else(|| goods.base_unit_code.clone()),
                unit_price_gross: goods.unit_price_gross.to_string(),
                gross_amount: line.gross_amount.to_string(),
            })
        })
        .collect()
}

/// 安全目录只暴露文件展示字段；资产版本和 HMAC 留在服务器允许清单。
///
/// # 参数
/// * `file` - 审批材料文件。
/// * `contract_file` - 合同 PDF 的文件资产身份；匹配时种类为 `CONTRACT`。
///
/// # 返回
/// 返回文件展示字段。
///
/// # 错误
/// 不返回错误。
pub(super) fn material_view(
    file: &ApprovalMaterialFile,
    contract_file: Option<&str>,
) -> PurchaseSourceSalesMaterialView {
    PurchaseSourceSalesMaterialView {
        file_asset_id: file.file_asset_id.to_string(),
        kind: if contract_file == Some(file.file_asset_id.as_ref()) { "CONTRACT" } else { "EVIDENCE" }.into(),
        file_name: file.file_name.clone(),
        content_type: file.content_type.clone(),
        byte_size: file.byte_size,
    }
}

/// 关系缺失使用完整性错误，不把缺失成交价解释为零价。
fn missing_relation() -> Error {
    Error::BusinessLogicError("采购关联销售明细缺失或不一致，请联系经办人核对".into())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{FileAssetId, SalesOrderLineId, SalesOrderRevisionLineId, SkuId, SkuRevisionId};
    use erp_sales::entity::sales_order::SalesPricingMode;

    use super::*;

    /// 构造明确所属销售版本和稳定行的冻结销售公共行。
    fn sales_line(id: &str, revision: &str, stable: &str) -> SalesOrderRevisionLine {
        let mut base = BaseModel::fake();
        base.id = id.into();
        SalesOrderRevisionLine {
            base,
            sales_order_revision_id: SalesOrderRevisionId::new(revision),
            sales_order_line_id: SalesOrderLineId::new(stable),
            line_no: 1,
            line_type: LineType::GoodsService,
            gross_amount: "123.46".parse().unwrap(),
            net_amount: "123.46".parse().unwrap(),
            tax_amount: "0".parse().unwrap(),
            sales_tax_rate: "0".parse().unwrap(),
            item_name_snapshot: "商品".into(),
            spec_snapshot: Some("规格".into()),
            unit_snapshot: Some("件".into()),
        }
    }

    /// 构造采购商品行引用；金额字段不参与销售价选择。
    fn purchase_line(revision_line: &str, stable: &str) -> PurchaseOrderLineView {
        PurchaseOrderLineView {
            line_id: "purchase-line".into(),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name: None,
            specification: None,
            quantity: Some("1".into()),
            base_unit_code: None,
            unit_cost_gross: Some("2.0000".into()),
            input_tax_rate: None,
            gross_amount: "2.00".into(),
            net_amount: "2.00".into(),
            tax_amount: "0.00".into(),
            expected_delivery_date: None,
            sales_order_line_id: Some(stable.into()),
            sales_order_revision_line_id: Some(revision_line.into()),
            sales_order_submission_line_id: None,
            allocated_quantity: Some("1".into()),
        }
    }

    #[test]
    fn chooses_rebound_revision_and_rejects_missing_or_cross_line_relationships() {
        let creation = SalesOrderRevisionId::new("old-revision");
        let sales = sales_line("new-line", "new-revision", "stable");
        let purchase = purchase_line("new-line", "stable");
        assert_eq!(
            source_revision_id(std::slice::from_ref(&purchase), std::slice::from_ref(&sales), &creation)
                .unwrap()
                .as_ref(),
            "new-revision"
        );
        assert!(source_revision_id(std::slice::from_ref(&purchase), &[], &creation).is_err());
        assert!(
            source_revision_id(
                &[purchase_line("new-line", "another-stable")],
                std::slice::from_ref(&sales),
                &creation
            )
            .is_err()
        );
        let other = sales_line("other-line", "other-revision", "other-stable");
        assert!(
            source_revision_id(
                &[purchase, purchase_line("other-line", "other-stable")],
                &[sales, other],
                &creation
            )
            .is_err()
        );
    }

    #[test]
    fn logistics_only_uses_creation_revision_without_creating_a_sales_price() {
        let mut purchase = purchase_line("ignored", "ignored");
        purchase.line_type = PurchaseLineType::LogisticsFee;
        purchase.sales_order_revision_line_id = None;
        let revision = SalesOrderRevisionId::new("creation");
        assert_eq!(source_revision_id(&[purchase], &[], &revision).unwrap(), revision);
    }

    #[test]
    fn actual_sales_price_joins_exact_revision_line_and_preserves_decimal_precision() {
        let line = sales_line("frozen-line", "revision", "stable");
        let mut goods = SalesOrderGoodsServiceLineRevision {
            base: BaseModel::fake(),
            revision_line_id: SalesOrderRevisionLineId::new("frozen-line"),
            sku_id: SkuId::new("sku"),
            sku_revision_id: SkuRevisionId::new("sku-revision"),
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: Instant::now(),
            quantity: "1".parse().unwrap(),
            base_unit_code: "piece".into(),
            unit_price_gross: "123.4567".parse().unwrap(),
            pricing_mode: SalesPricingMode::Manual,
        };
        let views = sales_line_views(std::slice::from_ref(&line), std::slice::from_ref(&goods)).unwrap();
        assert_eq!(views[0].unit_price_gross, "123.4567");
        assert_eq!(views[0].unit, "件");
        assert_eq!(views[0].sales_order_revision_line_id, "frozen-line");
        goods.revision_line_id = SalesOrderRevisionLineId::new("different-revision-line");
        assert!(sales_line_views(&[line], &[goods]).is_err());
    }

    #[test]
    fn material_directory_does_not_expose_storage_or_server_verification_fields() {
        let file = ApprovalMaterialFile {
            file_asset_id: FileAssetId::new("asset"),
            file_name: "合同.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 10,
            asset_version: 4,
            content_hmac: "a".repeat(64),
        };
        let value = serde_json::to_value(material_view(&file, Some("asset"))).unwrap();
        assert_eq!(value["kind"], "CONTRACT");
        assert!(value.get("content_hmac").is_none());
        assert!(value.get("asset_version").is_none());
        assert_eq!(material_view(&file, None).kind, "EVIDENCE");
    }
}
