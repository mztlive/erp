//! 采购内容精确关联的销售版本；普通销售及合同读取权限不参与此上下文授权。

mod mapping;
mod material;

use std::collections::BTreeSet;

use erp_core::ids::{SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::dto::purchase_order::{PurchaseOrderLineView, TotalsView};
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_sales::entity::sales_order::SalesOrderRevision;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
pub use material::PurchaseSalesMaterialReference;
use mongodb::Database;
use persistence_core::Executor;

use super::dto::{
    PurchaseSourceSalesLineView, PurchaseSourceSalesMaterialView, PurchaseSourceSalesOrderView,
};
use crate::sales_center::materials::repository::revision_lines;
use crate::sales_center::materials::revision_materials;
use crate::{Error, Result};

/// 沿采购行精确销售版本引用加载销售正文、价格和允许材料，不读取当前销售内容。
///
/// # 参数
/// * `db` - 目标数据库。
/// * `order` - 采购单。
/// * `purchase_lines` - 已投影的采购行。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回精确销售版本上的正文、成交价和允许材料；材料目录不可读时 `materials_unavailable` 为真且材料为空。
///
/// # 错误
/// 销售版本关系不一致、来源销售单或版本不存在、版本不属于来源销售单，或销售行与账号读取失败时返回对应错误。
pub(super) async fn source_sales(
    db: &Database,
    order: &PurchaseOrder,
    purchase_lines: &[PurchaseOrderLineView],
    executor: &mut dyn Executor,
) -> Result<PurchaseSourceSalesOrderView> {
    let revision = source_revision(db, order, purchase_lines, executor).await?;
    let sales = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购来源销售单不存在".into()))?;
    let names =
        db.accounts().names_by_ids(std::slice::from_ref(&sales.sales_owner_user_id), executor).await?;
    let lines = sales_lines(db, &revision, executor).await?;
    let (materials, materials_unavailable) = source_materials(db, &revision, executor).await;
    let view = PurchaseSourceSalesOrderView {
        sales_order_id: sales.base.id,
        sales_order_no: sales.order_no,
        status: sales.commercial_status.as_str().into(),
        revision_id: revision.base.id,
        revision_no: revision.revision.revision_no,
        customer_name: revision.customer_snapshot.customer_name,
        sales_owner_name: names.get(&sales.sales_owner_user_id).cloned(),
        contract_no: revision.contract_snapshot.map(|contract| contract.contract_no),
        totals: TotalsView {
            gross: revision.gross_amount.to_string(),
            net: revision.net_amount.to_string(),
            tax: revision.tax_amount.to_string(),
        },
        lines,
        materials,
        materials_unavailable,
    };
    Ok(view)
}

/// 文件治理异常仅关闭材料目录，不抹去已经精确证明的销售正文和成交价。
async fn source_materials(
    db: &Database,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> (Vec<PurchaseSourceSalesMaterialView>, bool) {
    match revision_materials(db, revision, executor).await {
        Ok(materials) => {
            let contract_file =
                materials.contract.as_ref().map(|contract| contract.contract_pdf_file_id.as_ref());
            (materials.files.iter().map(|file| mapping::material_view(file, contract_file)).collect(), false)
        },
        Err(error) => {
            tracing::warn!(sales_order_revision_id = %revision.base.id, error = %error, "采购关联销售材料暂不可读取");
            (Vec::new(), true)
        },
    }
}

/// 显示内容引用优先于建单版本；采购生效时行引用可能重绑为当时的销售版本。
async fn source_revision(
    db: &Database,
    order: &PurchaseOrder,
    purchase_lines: &[PurchaseOrderLineView],
    executor: &mut dyn Executor,
) -> Result<SalesOrderRevision> {
    let ids = purchase_lines
        .iter()
        .filter_map(|line| line.sales_order_revision_line_id.as_ref())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let referenced = revision_lines(db, &ids, executor).await?;
    let revision_id =
        mapping::source_revision_id(purchase_lines, &referenced, &order.sales_order_revision_id)?;
    let revision = db
        .sales_order_revisions()
        .find_by_id(&revision_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购关联销售版本不存在".into()))?;
    if revision.sales_order_id != order.sales_order_id {
        return Err(Error::BusinessLogicError("采购关联销售版本不属于来源销售单".into()));
    }
    Ok(revision)
}

/// 批量读取同一销售版本实物服务子行，成交价格仅取该不可变销售子行。
async fn sales_lines(
    db: &Database,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseSourceSalesLineView>> {
    let lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(&revision.base.id), executor)
        .await?;
    let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
    let goods =
        db.sales_order_goods_service_line_revisions().list_by_revision_line_ids(&ids, executor).await?;
    mapping::sales_line_views(&lines, &goods)
}
