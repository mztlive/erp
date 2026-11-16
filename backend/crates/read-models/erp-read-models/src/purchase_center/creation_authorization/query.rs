//! 采购创建任务的授权编排；查询与复验沿同一读取事务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_permissions::purchase_order;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::purchase_center::repository::creation_tasks::{CreationTaskFilter, read_creation_tasks};

/// 在调用方快照中完成任务查询及当前阶段交付检查。
/// # 参数
/// db、rbac、actor 为共享设施与当前账号；filter 只收窄目标，executor 为原执行器。
/// # 返回
/// 完整授权后的开放任务。
/// # 错误
/// 功能资格、阶段、有效期或仓储读取失败时拒绝。
pub(crate) async fn query_tasks(
    db: &Database,
    rbac: &Arc<AuthorizationService>,
    actor: &AuditActor,
    filter: CreationTaskFilter,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    let access = rbac.query(actor, &purchase_order::CREATE, executor).await?;
    let tasks = read_creation_tasks(db, &filter, &access, executor).await?;
    rbac.validate(&access, executor).await?;
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unbound_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    #[tokio::test]
    async fn creation_tasks_reject_missing_mapping_before_loading_any_business_task() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("creation_task_authorization");
        let actor = AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin);
        let result = query_tasks(
            &db,
            &unbound_authorization(),
            &actor,
            CreationTaskFilter { sales_order_id: None, work_item_id: None },
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(result, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"));
    }
}
