//! 选品访问器复用启动时完成绑定的统一授权服务。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_sales::service::sales_selection::SelectionAccess;
use mongodb::Database;

/// 装配选品对象、来源和查询检查入口。
/// 不保存请求状态的选品访问器。
pub fn selection_access(db: Database, authorization: Arc<AuthorizationService>) -> SelectionAccess {
    SelectionAccess::new(db, authorization)
}
