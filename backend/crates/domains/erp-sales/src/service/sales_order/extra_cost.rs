//! 附加费用归属的销售单与销售明细资格。

use erp_core::ids::{SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_order::{LineType, SalesOrder};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 校验归属明细在销售当前生效版本中且数量大于 0。
/// 结果：明细可归属时返回 `Ok(())`。
/// 失败条件：明细不在当前版本、不是实物与服务明细或数量为 0 时返回 `ValidationError`；仓储失败返回对应错误。
///
/// 输入约定：
/// - `order`：已生效销售单
/// - `line_id`：归属销售明细；为空表示整张销售单，直接通过
/// - `executor`：调用方事务执行器
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
