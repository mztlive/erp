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
        return Err(Error::BusinessLogicError("此订单由采购 API 流程自动执行和跟踪，请使用接口配送资料及来源采购处理；不能通过旧发单、重放或人工回执入口修改".into()));
    }
    Ok(())
}
