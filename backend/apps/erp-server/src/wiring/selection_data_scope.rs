//! 选品固定授权能力的组合根装配。
use erp_identity::SharedRbacService;
use erp_sales::service::sales_selection::SelectionAccess;
use mongodb::Database;

/// 构造选品统一授权访问器。
/// # 参数
/// db、rbac 为应用共享数据库与身份设施。
/// # 返回
/// 无跨请求授权缓存的选品访问器。
/// # 错误
/// 固定目录非法时启动失败；不执行 I/O。
pub fn selection_access(db: Database, rbac: SharedRbacService) -> SelectionAccess {
    SelectionAccess::new(db, rbac)
}
