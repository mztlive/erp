//! 组织停用前核验尚未结清的销售业务。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use super::prelude::*;
use super::{SalesOrderExt, SalesSelectionExt};

/// 销售单或仍在准备、发布中的选品册须先完成交接。
///
/// # 错误
/// 传播数据库读取错误，不将查询失败视作没有未结业务。
pub async fn has_unsettled_business_org(
    db: &Database,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if db.sales_orders().has_unsettled_business_org(org, executor).await? {
        return Ok(true);
    }
    db.sales_selection_booklets()
        .exists(
            doc! {
                "business_org_unit_id": org,
                "status": { "$nin": ["SUBMITTED", "CLOSED", "VOIDED"] }
            },
            executor,
        )
        .await
}
