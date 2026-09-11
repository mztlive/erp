//! 选品内部入口复用客户领域的有效归属政策。
use super::super::customer::{ensure_customer_access, has_permission};
use crate::{
    app_state::AppState,
    core::{errors::Error, middleware::RbacSubject},
};

/// 解析列表允许访问的客户集合。全量权限必须在服务端验证。
/// # 错误
/// 客户归属查询失败时拒绝请求。
pub(super) async fn customer_ids(
    state: &AppState,
    subject: &RbacSubject,
    user: &str,
) -> Result<Option<Vec<String>>, Error> {
    let mut scope = erp_customer::CustomerScope::Assigned;
    if has_permission(state, subject, "customer_scope:detail").await? {
        scope = erp_customer::CustomerScope::AllAuthorized;
    }
    Ok(state
        .customer_service()
        .customer_ids_for_scope(scope, user)
        .await?)
}

/// 按册定位客户，再执行现有客户权限校验；包含幂等重放入口。
/// # 错误
/// 册不存在、无客户权限或查询失败时拒绝请求。
pub(super) async fn booklet(
    state: &AppState,
    subject: &RbacSubject,
    user: &str,
    id: &str,
) -> Result<(), Error> {
    let view = super::process(state).booklet_detail(id).await?;
    ensure_customer_access(state, subject, user, &view.customer_id).await
}
