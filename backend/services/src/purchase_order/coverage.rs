//! 销售当前版本的采购数量覆盖编排。
//!
//! 草稿、旧待财务与审批中采购只读取 `current_submission_id`；生效、部分执行与
//! 已完成采购只读取 `current_revision_id` 及其销售分配。历史提交、历史采购版本
//! 与作废采购单均不进入覆盖量。
//!
//! 事实批量加载由 `database::PurchaseOrderExt::load_procurement_coverage_facts`
//! 承担；覆盖聚合、累计、当前行关联、超覆盖拒绝、剩余量与进度计算由
//! `entities::purchase_order::coverage::build_procurement_coverage` 领域构造函数
//! 承担；本模块只负责当前指针解析、仓储调用与领域错误映射。

use database::{Executor, PurchaseOrderExt};
use entities::ids::SalesOrderRevisionId;
use entities::purchase_order::{build_procurement_coverage, SalesProcurementCoverage};
use entities::sales_order::SalesOrder;

use crate::errors::{Error, Result};

/// 加载销售单当前版本及采购覆盖数量。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `order` - 已加载的销售稳定单
/// * `executor` - 数据访问执行器；创建命令必须传入事务会话
///
/// # 返回
/// 返回当前销售版本商品/服务目标行、逐行覆盖与总汇总。
///
/// # 错误
/// 当前版本缺失、当前采购指针缺失、正式分配未绑定销售当前版本行、覆盖超过目标
/// 或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 只沿销售与采购的当前指针读取，稳定关联键为 `sales_order_line_id`；事务内
/// 调用必须复用调用方 executor，保证与同事务写入的 read-your-writes。
pub(crate) async fn load_sales_procurement_coverage(
    db: &mongodb::Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<SalesProcurementCoverage> {
    let revision_id = order
        .stable
        .current_revision_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法计算采购剩余量".to_string()))?;
    let facts = db
        .load_procurement_coverage_facts(
            &SalesOrderRevisionId::new(revision_id.clone()),
            &order.base.id.clone().into(),
            executor,
        )
        .await?;
    build_procurement_coverage(facts).map_err(Error::Logic)
}
