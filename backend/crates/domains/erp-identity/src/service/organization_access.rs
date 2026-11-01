//! 调用方事务内的组织启用资格检查。
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::organization::OrgTree;
use crate::repository::OrganizationRepository;
use crate::{Error, Result};

/// 校验显式目标组织整条路径均启用。
///
/// # 参数
/// * `db` - 目标数据库
/// * `target_org` - 可选目标组织
/// * `executor` - 调用方执行器
///
/// # 返回
/// 未指定组织或路径全部启用时成功。
///
/// # 错误
/// 目标组织停用时拒绝。
///
/// # 关键业务约束
/// 只做启用检查，不提供任何范围授权；需拒绝公司组织的调用方先自检。
pub async fn ensure_org_enabled(
    db: &Database,
    target_org: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".into()));
    }
    Ok(())
}
