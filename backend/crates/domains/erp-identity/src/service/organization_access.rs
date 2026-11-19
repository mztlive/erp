//! 调用方事务内的组织启用资格检查。
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::organization::OrgTree;
use crate::repository::OrganizationRepository;
use crate::{Error, Result};

/// 业务写入引用显式组织前，确认目标到根的整条路径均启用；空目标直接通过。调用方另行检查范围授权及是否允许公司级目标。
pub async fn ensure_org_enabled(
    db: &Database,
    target_org: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let units = OrganizationRepository::new(db).units(executor).await?;
    let tree = OrgTree::new(&units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".into()));
    }
    Ok(())
}
