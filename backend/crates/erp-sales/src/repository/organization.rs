//! 组织停用前核验尚未结清的销售业务。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use super::prelude::*;
use super::{SalesOrderExt, SalesSelectionExt};

/// 判断业务组织是否仍有未作废且未关闭的销售单，或尚未提交、关闭、作废的选品册。
///
/// # 参数
/// * `db` - 目标数据库。
/// * `org` - 业务组织 ID。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 销售单命中，或选品册 `status` 不在 `SUBMITTED`、`CLOSED`、`VOIDED` 时返回 `true`。
/// 销售单已命中时不再查询选品册。
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
