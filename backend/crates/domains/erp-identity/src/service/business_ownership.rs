//! 责任账号的有效主属组织资格；业务归属快照由消费方拥有。

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::organization::OrgTree;
use crate::repository::OrganizationRepository;
use crate::{AccessControlExt, Error, Result};

/// 解析有效后台责任人的唯一主属组织，不默认放入根组织或系统账号。
///
/// # 参数
/// * `db` - 账号与组织所在数据库。
/// * `user` - 业务负责人账号。
/// * `executor` - 调用方读或事务执行器。
///
/// # 返回
/// 返回仍启用的主属组织 ID。
///
/// # 错误
/// 账号不是有效后台账号、缺少有效主属组织、组织树无法构造或所属组织已停用时返回业务错误。仓储失败时返回对应错误。
pub async fn required_business_org(db: &Database, user: &str, executor: &mut dyn Executor) -> Result<String> {
    db.accounts()
        .find_by_id(user, executor)
        .await?
        .filter(|a| a.is_active_backoffice())
        .ok_or_else(|| Error::BusinessLogicError("业务负责人必须为有效后台账号".into()))?;

    let state = OrganizationRepository::new(db).directory_state(user, Instant::now(), executor).await?;
    let org = state
        .own_org(user, Instant::now())?
        .ok_or_else(|| Error::BusinessLogicError("负责人缺少有效主属组织，请先维护组织成员".into()))?;
    if OrgTree::new(&state.units)?.expand(org, false)?.is_empty() {
        return Err(Error::BusinessLogicError("负责人所属组织已停用".into()));
    }
    Ok(org.into())
}

/// 在正式写入事务中重验预先采集的组织，调岗后不得提交旧建单计划。
///
/// # 参数
/// * `db` - 账号与组织所在数据库。
/// * `user` - 业务负责人账号。
/// * `org` - 预先采集的主属组织 ID。
/// * `executor` - 正式写入事务的执行器。
///
/// # 返回
/// 当前主属组织仍等于 `org` 时返回。
///
/// # 错误
/// 责任事实失效时返回 `required_business_org` 的错误；组织已变化时返回冲突。
pub async fn ensure_creation_org(
    db: &Database,
    user: &str,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if required_business_org(db, user, executor).await? != org {
        return Err(Error::ConflictError("负责人主属组织已变化，请重新创建".into()));
    }
    Ok(())
}
