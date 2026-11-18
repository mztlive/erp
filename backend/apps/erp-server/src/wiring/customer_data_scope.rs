//! 客户固定授权能力的组合根装配。
use std::sync::Arc;

use erp_customer::CustomerAccess;
use erp_identity::AuthorizationService;
use mongodb::Database;

/// 构造客户统一授权访问器。
/// 使用共享统一授权服务且无跨请求授权缓存的访问器。
pub fn customer_access(db: Database, authorization: Arc<AuthorizationService>) -> CustomerAccess {
    CustomerAccess::new(db, authorization)
}
