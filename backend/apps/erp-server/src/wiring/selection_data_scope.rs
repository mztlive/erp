//! 选品访问器复用启动时完成绑定的统一授权服务。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_sales::service::sales_selection::SelectionAccess;
use mongodb::Database;

/// 装配选品对象、来源和查询检查入口。
/// # 参数
/// db 为原业务数据库，authorization 为进程共享统一授权服务。
/// # 返回
/// 不保存请求状态的选品访问器。
/// # 错误
/// 构造不执行 I/O。
pub fn selection_access(db: Database, authorization: Arc<AuthorizationService>) -> SelectionAccess {
    SelectionAccess::new(db, authorization)
}
