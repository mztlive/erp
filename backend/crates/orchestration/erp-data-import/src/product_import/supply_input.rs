//! 商品与供给联合导入的输入 DTO；采购成本与公司销售参考价分开解释。

use erp_core::common::time::BusinessDate;
use erp_core::money::Rate;
use erp_supply::dto::supplier_offering::{CreateSupplierOfferingRequest, SupplierOfferingTermsWrite};
use erp_supply::entity::supplier_offering::write_data::{parse_minimum_order_quantity, parse_unit_price};
use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingSourceType};
use rust_decimal::Decimal;

use super::identity::stable_code;
use crate::{Error, Result};

/// 当前导入行的供应商身份与已校验条款。
pub(super) struct SupplyImportInput {
    pub supplier_no: String,
    pub supplier_sku_code: String,
    pub terms: SupplierOfferingTermsWrite,
}

impl SupplyImportInput {
    /// 从 16 列模板读取采购条款；空起订量默认为 1，税率以百分数填写。
    pub(super) fn from_cells(cells: &[String], today: BusinessDate) -> Result<Self> {
        let cell = |index: usize| cells.get(index).map(|value| value.trim()).unwrap_or("");
        for (index, label) in [(13, "供应商编号"), (14, "供应商订货编码"), (15, "进项税率（%）")]
        {
            if cell(index).is_empty() {
                return Err(Error::ValidationError(format!("{label}不能为空")));
            }
        }
        let dropship = parse_unit_price(cell(9), "供应商代发成本价")?;
        let bulk = parse_unit_price(cell(10), "供应商集采成本价")?;
        if dropship.to_decimal() < Decimal::ZERO || bulk.to_decimal() < Decimal::ZERO {
            return Err(Error::ValidationError("供应商成本价不能为负数".into()));
        }
        let minimum = parse_minimum_order_quantity(if cell(11).is_empty() { "1" } else { cell(11) })?;
        if minimum.to_decimal() <= Decimal::ZERO {
            return Err(Error::ValidationError("供应商集采起订量必须大于 0".into()));
        }
        let percent =
            cell(15).trim_end_matches('%').trim().parse::<Decimal>().map_err(|_| {
                Error::ValidationError("进项税率须填写 0–100 的百分数，例如 13 表示 13%".into())
            })?;
        if percent < Decimal::ZERO || percent > Decimal::from(100) {
            return Err(Error::ValidationError("进项税率须在 0–100 之间".into()));
        }
        let rate = Rate::try_from(percent / Decimal::from(100))?;
        Ok(Self {
            supplier_no: cell(13).to_string(),
            supplier_sku_code: cell(14).to_string(),
            terms: SupplierOfferingTermsWrite::new(
                dropship.to_string(),
                bulk.to_string(),
                rate.to_string(),
                minimum.to_string(),
                today.to_string(),
            ),
        })
    }

    /// 为目标 SKU 构造供给命令，999 仅作为首次创建的可供数量。
    pub(super) fn request(&self, supplier_id: &str, sku_id: &str) -> CreateSupplierOfferingRequest {
        CreateSupplierOfferingRequest {
            sku_id: sku_id.to_string(),
            supplier_id: supplier_id.to_string(),
            supplier_product_code: None,
            supplier_sku_code: self.supplier_sku_code.clone(),
            source_type: OfferingSourceType::Excel,
            source_connection_id: None,
            terms: self.terms.clone(),
            delivery_template_id: None,
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some("999".into()),
            source_updated_at: None,
            source_revision_token: None,
            change_reason: "商品导入建立供应商供给".into(),
            idempotency_key: stable_code("IMPORT-SUPPLY", &[supplier_id, &self.supplier_sku_code, sku_id]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells() -> Vec<String> {
        let mut cells = vec![String::new(); 16];
        cells[9] = "31.1234".into();
        cells[10] = "29.50".into();
        cells[13] = "SUP-001".into();
        cells[14] = "RICE-5KG".into();
        cells[15] = "13".into();
        cells
    }

    #[test]
    fn supplier_prices_tax_and_initial_quantity_are_explicit() {
        let input = SupplyImportInput::from_cells(&cells(), "2026-10-26".parse().unwrap()).unwrap();
        let request = input.request("supplier-1", "sku-1");
        assert_eq!(request.terms.dropship_supply_price_gross, "31.1234");
        assert_eq!(
            request.terms.bulk_supply_price_gross.as_deref().unwrap().parse::<Decimal>().unwrap(),
            Decimal::new(295, 1)
        );
        assert_eq!(request.terms.input_tax_rate, "0.13");
        assert_eq!(request.terms.bulk_minimum_order_quantity.as_deref().unwrap(), "1");
        assert_eq!(request.available_quantity.as_deref(), Some("999"));
        assert_eq!(request.availability_status, AvailabilityStatus::Available);
        assert_eq!(request.source_type, OfferingSourceType::Excel);
        assert_eq!(request.terms.valid_from, "2026-10-26");
        assert!(request.terms.valid_to.is_none());
        assert!(request.delivery_template_id.is_none());
    }

    #[test]
    fn missing_identity_prices_tax_and_invalid_quantities_fail() {
        for (index, value) in [
            (13, ""),
            (14, ""),
            (9, ""),
            (10, ""),
            (15, ""),
            (9, "-1"),
            (10, "1.00001"),
            (11, "0"),
            (11, "-1"),
            (15, "101"),
            (15, "-1"),
            (15, "bad"),
            (15, "0.00001"),
        ] {
            let mut row = cells();
            row[index] = value.into();
            assert!(
                SupplyImportInput::from_cells(&row, "2026-10-26".parse().unwrap()).is_err(),
                "{index}: {value}"
            );
        }
    }
}
