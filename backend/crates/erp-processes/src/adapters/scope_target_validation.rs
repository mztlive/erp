//! 范围配置显式目标的身份校验；仓库和结算主体不进入内部组织树。
use async_trait::async_trait;
use erp_identity::service::access_control::AccessControlService;
use erp_identity::{
    access_control::ScopeDimension, ports::ScopeTargetPort, Error, Result, SharedRbacService,
};
use erp_party::PartyExt;
use erp_warehouse::WarehouseExt;
use mongodb::Database;
use persistence_core::Executor;
use std::{collections::HashSet, sync::Arc};

struct ScopeTargets(Database);

#[async_trait]
impl ScopeTargetPort for ScopeTargets {
    async fn validate_targets(
        &self,
        dimension: ScopeDimension,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let found = match dimension {
            ScopeDimension::Warehouse => self
                .0
                .warehouses()
                .list_active_by_ids(ids, executor)
                .await?
                .into_iter()
                .map(|row| row.base.id)
                .collect::<HashSet<_>>(),
            ScopeDimension::SettlementParty => self
                .0
                .parties()
                .list_active_by_ids(ids, executor)
                .await?
                .into_iter()
                .map(|row| row.base.id)
                .collect::<HashSet<_>>(),
            ScopeDimension::InternalOrg => {
                return Err(Error::ValidationError("内部组织必须由组织域校验".into()))
            }
        };
        if ids.iter().any(|id| !found.contains(id)) {
            return Err(Error::ValidationError(
                "范围目标在对应身份域不存在或已删除".into(),
            ));
        }
        Ok(())
    }
}

/// 装配范围配置的公共授权与外部目标校验。
///
/// # 参数
/// * `db` - 业务数据库
/// * `rbac` - 现有 RBAC 服务
/// # 返回
/// 返回无嵌套事务的配置服务。
/// # 错误
/// 无；目标错误由创建命令在原事务中返回。
pub fn scope_configuration(db: Database, rbac: SharedRbacService) -> AccessControlService {
    AccessControlService::new(db.clone())
        .with_rbac(rbac)
        .with_scope_targets(Arc::new(ScopeTargets(db)))
}
