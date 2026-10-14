//! 附加费用归属的销售单与销售明细资格。

use erp_core::ids::{SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_sales::entity::sales_order::{BusinessType, CommercialStatus, LineType, SalesOrder};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 校验销售单可登记附加费用：必须为已生效的实物与服务销售单。
///
/// # 参数
/// * `order` - 已通过对象范围校验的销售单
///
/// # 返回
/// 可登记时返回 `Ok(())`。
///
/// # 错误
/// 卡券销售单或未生效销售单返回 `BusinessLogicError`。
pub fn ensure_extra_cost_order(order: &SalesOrder) -> Result<()> {
    if order.business_type != BusinessType::GoodsService {
        return Err(Error::BusinessLogicError("卡券销售单不登记附加费用".into()));
    }
    if order.commercial_status != CommercialStatus::Effective {
        return Err(Error::BusinessLogicError("销售单生效后才能登记附加费用".into()));
    }
    Ok(())
}

/// 校验归属明细在销售当前生效版本中且数量大于 0。
///
/// # 参数
/// * `db` - 数据库
/// * `order` - 已生效销售单
/// * `line_id` - 归属销售明细；为空表示整张销售单，直接通过
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 明细可归属时返回 `Ok(())`。
///
/// # 错误
/// 明细不在当前版本、不是实物与服务明细或数量为 0 时返回 `ValidationError`；仓储失败返回对应错误。
pub async fn ensure_attributable_line(
    db: &Database,
    order: &SalesOrder,
    line_id: Option<&SalesOrderLineId>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(line_id) = line_id else {
        return Ok(());
    };
    let revision_id = order
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前生效版本".into()))?;
    let lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(revision_id), executor)
        .await?;
    let line = lines
        .iter()
        .find(|line| &line.sales_order_line_id == line_id && line.line_type == LineType::GoodsService)
        .ok_or_else(invalid_line)?;
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&[SalesOrderRevisionLineId::new(line.base.id.clone())], executor)
        .await?;
    match goods.first() {
        Some(goods) if goods.quantity.to_decimal() > rust_decimal::Decimal::ZERO => Ok(()),
        _ => Err(invalid_line()),
    }
}

fn invalid_line() -> Error {
    Error::ValidationError("归属明细须为销售单当前生效版本中数量大于 0 的明细".into())
}
