//! S2 业务责任采集与首次生效归属；组织变更不回写业务责任。

use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_sales::entity::sales_order::{AttributionOrgNode, SalesAttribution, SalesOrder};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 解析有效后台责任人的唯一主属组织，不默认放入根组织或系统账号。
///
/// # 错误
/// 账号、成员或组织事实缺失及失效时阻断业务创建。
pub async fn required_business_org(db: &Database, user: &str, executor: &mut dyn Executor) -> Result<String> {
    db.accounts()
        .find_by_id(user, executor)
        .await?
        .filter(|a| a.is_active_backoffice())
        .ok_or_else(|| Error::BusinessLogicError("业务负责人必须为有效后台账号".into()))?;
    let state = OrganizationRepository::new(db).state(executor).await?;
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
/// # 错误
/// 组织变化或责任事实失效时拒绝并由根事务回滚。
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

/// 按单据负责销售和业务组织采集历史归属，组织路径包含根至当前节点。
///
/// # 错误
/// 缺账号、业务组织或完整路径时拒绝首次生效，不按人员当前组织补写。
pub async fn sales_attribution(
    db: &Database,
    order: &SalesOrder,
    at: Instant,
    executor: &mut dyn Executor,
) -> Result<SalesAttribution> {
    let account = db
        .accounts()
        .find_by_id(&order.sales_owner_user_id, executor)
        .await?
        .filter(|a| a.is_active_backoffice())
        .ok_or_else(|| Error::BusinessLogicError("首次生效的负责销售已失效".into()))?;
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(&order.business_org_unit_id)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("单据业务组织已停用，必须先完成交接".into()));
    }
    let org = path.last().ok_or_else(|| Error::BusinessLogicError("单据业务组织缺失".into()))?;
    let snapshot = SalesAttribution {
        attribution_user_id: account.base.id,
        attribution_user_name: account.name,
        attribution_org_unit_id: org.base.id.clone(),
        attribution_org_unit_name: org.name.clone(),
        attributed_at: at,
        attribution_version: 1,
        organization_version: state.version,
        org_path: path
            .iter()
            .map(|node| AttributionOrgNode { id: node.base.id.clone(), name: node.name.clone() })
            .collect(),
    };
    snapshot.validate(&order.sales_owner_user_id, &order.business_org_unit_id)?;
    Ok(snapshot)
}

/// 生效事务重验准备阶段的归属事实，快照失败时不允许提交单据生效。
///
/// # 错误
/// 人员更名、组织移动或责任事实变化均要求重新准备生效计划。
pub async fn ensure_attribution(
    db: &Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let frozen =
        order.attribution.as_ref().ok_or_else(|| Error::BusinessLogicError("销售生效归属快照缺失".into()))?;
    let current = sales_attribution(db, order, frozen.attributed_at, executor).await?;
    if current != *frozen {
        return Err(Error::ConflictError("销售归属事实已变化，请重新提交生效".into()));
    }
    Ok(())
}
