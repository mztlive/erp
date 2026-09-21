//! 组织停用所需的业务未结事实；不改变任务责任和业务归属。

use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::ports::OrganizationBusinessPort;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
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
        if erp_sales::repository::organization::has_unsettled_business_org(&self.db, org, executor).await? {
            return Ok(true);
        }
        if self.db.purchase_orders().has_unsettled_business_org(org, executor).await?
            || erp_supply::repository::organization::has_unsettled_business_org(&self.db, org, executor)
                .await?
            || erp_integration::repository::organization::has_unsettled_business_org(&self.db, org, executor)
                .await?
        {
            return Ok(true);
        }
        Ok(false)
    }
}

/// 装配组织管理与销售、采购、供应及集成未结业务检查。
///
/// # 返回
/// 返回必须通过真实业务事实核验才可停用组织的服务。
pub fn organization_service(
    db: Database,
    rbac: erp_identity::SharedRbacService,
) -> erp_identity::service::organization::OrganizationService {
    erp_identity::service::organization::OrganizationService::new(
        db.clone(),
        rbac,
        Arc::new(OrganizationBusinessFacts { db }),
    )
}
