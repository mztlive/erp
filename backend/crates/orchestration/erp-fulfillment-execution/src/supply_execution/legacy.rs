//! 旧动作入口的 API 执行排除检查，沿授权事务读取。
use erp_supply::api_order::ApiOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 在当前授权事务中排除新采购 API 执行对象。
/// # 参数
/// db、id 和 executor 为已完成当前动作授权的原订单与执行器。
/// # 返回
/// 订单没有 API 自动执行记录时成功。
/// # 错误
/// 存在 API 执行记录或仓储读取失败时拒绝旧入口。
pub(super) async fn ensure(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<()> {
    if db.supplier_api_order_executions().find_by_id(id, executor).await?.is_some() {
        return Err(Error::BusinessLogicError("此订单由来源采购自动向供应商发单并跟踪，请从来源采购核对配送资料和办理进度，不要手工发单或补录结果".into()));
    }
    Ok(())
}
