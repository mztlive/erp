//! 组织停用所需的业务未结事实；不改变任务责任和业务归属。

use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::ports::OrganizationBusinessPort;
use erp_identity::service::organization::OrganizationService;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_integration::repository::organization::has_unsettled_business_org as integration_unsettled;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::repository::organization::has_unsettled_business_org as sales_unsettled;
use erp_supply::repository::organization::has_unsettled_business_org as supply_unsettled;
use mongodb::Database;
use persistence_core::Executor;

struct OrganizationBusinessFacts {
    db: Database,
}

#[async_trait]
impl OrganizationBusinessPort for OrganizationBusinessFacts {
    async fn has_unsettled_business(
        &self,
        org: &str,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<bool> {
        if sales_unsettled(&self.db, org, executor).await? {
            return Ok(true);
        }
        if self.db.purchase_orders().has_unsettled_business_org(org, executor).await?
            || supply_unsettled(&self.db, org, executor).await?
            || integration_unsettled(&self.db, org, executor).await?
        {
            return Ok(true);
        }
        Ok(false)
    }
}

/// 装配组织管理与销售、采购、供应及集成未结业务检查。
///
/// # 参数
/// * `db` - 各域未结业务集合所在数据库。
/// * `rbac` - 组织服务使用的 RBAC 服务。
/// * `authorization` - 完整绑定的共享授权服务。
///
/// # 返回
/// 返回必须通过真实业务事实核验才可停用组织的服务。
///
/// # 错误
/// 不返回错误。
pub fn organization_service(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> OrganizationService {
    OrganizationService::new(db.clone(), rbac, Arc::new(OrganizationBusinessFacts { db }), authorization)
}
