//! 按冻结责任事实读取销售首次生效归属，并核对原执行器内的版本。

use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_sales::entity::sales_order::{AttributionOrgNode, SalesAttribution, SalesOrder};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 按单据负责销售和业务组织采集历史归属，组织路径包含根至当前节点。
///
/// 不按人员当前组织补写。
///
/// # 参数
/// * `db` - 账号与组织所在数据库。
/// * `order` - 提供负责销售和业务组织的销售单。
/// * `at` - 归属采集时刻。
/// * `executor` - 调用方读或事务执行器。
///
/// # 返回
/// 返回含人员姓名、组织路径和组织版本的归属快照。
///
/// # 错误
/// 负责销售已失效、业务组织缺失或停用、组织树或路径无法解析，或快照与单据字段不一致时返回错误。仓储失败时返回对应错误。
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

/// 生效事务重验准备阶段的归属事实，快照不一致时不允许提交单据生效。
///
/// # 参数
/// * `db` - 账号与组织所在数据库。
/// * `order` - 已带归属快照的销售单。
/// * `executor` - 生效事务的执行器。
///
/// # 返回
/// 按原归属时刻重算的快照与冻结快照一致时返回。
///
/// # 错误
/// 缺少冻结快照、重算失败，或人员、组织、路径等事实已变化时返回错误。
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
