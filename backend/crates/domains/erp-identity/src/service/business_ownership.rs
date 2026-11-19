//! 责任账号的有效主属组织资格；业务归属快照由消费方拥有。

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::organization::OrgTree;
use crate::repository::OrganizationRepository;
use crate::{AccessControlExt, Error, Result};

/// 建立业务归属前，确认负责人是有效后台账号，并返回其唯一且仍启用的主属组织。资格或组织事实缺失时停止建单，仓储错误原样返回。
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

/// 在建单写事务中复核预先记录的负责人组织。当前主属组织已变更时返回冲突，要求调用方重新准备写入。
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
