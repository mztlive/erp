//! 提交与保存统一从当前公司 SKU 回填可信商品快照及单位精度。

use erp_catalog::entity::catalog::ProductKind;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::CatalogExt;
use erp_core::common::time::BusinessDate;
use erp_sales::entity::sales_inquiry::{InquiryFulfillmentKind, InquiryRequirementData};
use mongodb::Database;
use persistence_core::Executor;

use crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery;
use crate::{Error, Result};

/// 在询价授权后重验每个精确 SKU 修订，回填商品快照与单位精度。
///
/// # 参数
/// `db` 为公司目录库，`data` 为待写工作副本，`executor` 为原事务。
/// # 返回
/// 所有行仍可售时更新可信快照；不改变稳定需求或地址身份。
/// # 错误
/// 无权选用或不存在、下架、修订失效、单位不符、卡券时统一拒绝。
pub async fn normalize_requirement(
    db: &Database,
    data: &mut InquiryRequirementData,
    executor: &mut dyn Executor,
) -> Result<()> {
    if data.lines.len() > 200 {
        return Err(Error::ValidationError("询价商品行不能超过 200 行".into()));
    }
    let refs =
        data.lines.iter().map(|line| (line.sku_id.clone(), line.sku_revision_id.clone())).collect::<Vec<_>>();
    let rows = MongoCatalogSupplyQuery::new(db.clone())
        .find_sellable_sku_refs(&refs, BusinessDate::today(), executor)
        .await?;
    for line in &mut data.lines {
        let row = rows
            .iter()
            .find(|row| row.sku_id == line.sku_id && row.sku_revision_id == line.sku_revision_id)
            .ok_or_else(unavailable)?;
        let unit =
            db.unit_of_measures().find_by_id(&row.base_unit_id, executor).await?.ok_or_else(unavailable)?;
        if !line.unit_code.is_empty() && line.unit_code != unit.unit_code {
            return Err(unavailable());
        }
        line.quantity_scale = u32::from(unit.quantity_scale);
        line.unit_code = unit.unit_code;
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

fn unavailable() -> Error {
    Error::ValidationError("商品不存在或当前不可选用，SKU 版本或单位已失效".into())
}
