//! 提交与保存统一从当前公司 SKU 回填可信商品快照及单位精度。

use erp_catalog::entity::catalog::{ProductKind, UnitOfMeasure};
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::CatalogExt;
use erp_core::common::time::BusinessDate;
use erp_sales::entity::sales_inquiry::{
    InquiryFulfillmentKind, InquiryRequirementData, InquiryRequirementLine,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 在询价授权后重验每个精确 SKU 修订，回填商品快照与单位精度。
///
/// `db` 为公司目录库，`data` 为待写工作副本，`executor` 为原事务。所有行仍可售时更新可信快照；不改变稳定需求或地址身份。
///
/// 无权选用或不存在、下架、修订失效、单位不符、卡券时统一拒绝。
pub async fn normalize_requirement(
    db: &Database,
    catalog: &dyn CatalogSupplyQueryPort,
    data: &mut InquiryRequirementData,
    executor: &mut dyn Executor,
) -> Result<()> {
    if data.lines.len() > 200 {
        return Err(Error::ValidationError("询价商品行不能超过 200 行".into()));
    }

    let refs =
        data.lines.iter().map(|line| (line.sku_id.clone(), line.sku_revision_id.clone())).collect::<Vec<_>>();
    let rows = catalog.find_sellable_sku_refs(&refs, BusinessDate::today(), executor).await?;

    for line in &mut data.lines {
        let row = rows
            .iter()
            .find(|row| row.sku_id == line.sku_id && row.sku_revision_id == line.sku_revision_id)
            .ok_or_else(unavailable)?;
        let unit =
            db.unit_of_measures().find_by_id(&row.base_unit_id, executor).await?.ok_or_else(unavailable)?;
        normalize_unit(line, unit)?;

        line.sku_code = row.sku_no.clone();
        line.product_name = row.product_name.clone().unwrap_or_else(|| row.name.clone());
        line.specification = row.specification.clone().unwrap_or_default();
        line.fulfillment_kind = match row.product_kind {
            ProductKind::Physical => InquiryFulfillmentKind::Physical,
            ProductKind::Virtual => InquiryFulfillmentKind::Virtual,
            ProductKind::OfflineService => InquiryFulfillmentKind::Service,
            ProductKind::Voucher => return Err(unavailable()),
        };
    }
    Ok(())
}

fn normalize_unit(line: &mut InquiryRequirementLine, unit: UnitOfMeasure) -> Result<()> {
    // 旧版选品把单位名称写入本地草稿。只接受该 SKU 基础单位的精确名称。
    if !line.unit_code.is_empty() && line.unit_code != unit.unit_code && line.unit_code != unit.name {
        return Err(unavailable());
    }

    line.quantity_scale = u32::from(unit.quantity_scale);
    line.unit_code = unit.unit_code;
    Ok(())
}

fn unavailable() -> Error {
    Error::ValidationError("商品不存在或当前不可选用，SKU 版本或单位已失效".into())
}

#[cfg(test)]
mod tests {
    use erp_catalog::entity::catalog::unit_of_measure::UnitOfMeasureData;
    use erp_core::common::time::Instant;
    use erp_core::ids::UnitOfMeasureId;

    use super::*;

    fn line(unit_code: &str) -> InquiryRequirementLine {
        InquiryRequirementLine {
            line_id: "line-1".into(),
            sku_id: "sku-1".into(),
            sku_revision_id: "sku-rev-1".into(),
            sku_code: "SKU-001".into(),
            product_name: "测试商品".into(),
            specification: String::new(),
            unit_code: unit_code.into(),
            quantity_scale: 6,
            quantity: "3".parse().unwrap(),
            fulfillment_kind: InquiryFulfillmentKind::Physical,
            delivery_due_at: Instant::from_unix_secs(0),
            delivery_terms: String::new(),
            sales_reference_price_gross: None,
            allocations: Vec::new(),
        }
    }

    fn box_unit() -> UnitOfMeasure {
        UnitOfMeasure::new(
            UnitOfMeasureId::new("unit-box"),
            UnitOfMeasureData::new("BOX", "盒", "盒"),
            "test",
        )
        .unwrap()
    }

    #[test]
    fn fills_canonical_unit_for_legacy_name_code_and_empty_value() {
        for input in ["盒", "BOX", ""] {
            let mut line = line(input);
            let quantity = line.quantity;
            normalize_unit(&mut line, box_unit()).unwrap();
            assert_eq!(line.unit_code, "BOX");
            assert_eq!(line.quantity_scale, 0);
            assert_eq!(line.quantity, quantity);
        }
    }

    #[test]
    fn rejects_code_or_name_from_a_different_unit() {
        for input in ["BAG", "袋"] {
            let mut line = line(input);
            assert!(matches!(normalize_unit(&mut line, box_unit()), Err(Error::ValidationError(_))));
            assert_eq!(line.unit_code, input);
        }
    }
}
