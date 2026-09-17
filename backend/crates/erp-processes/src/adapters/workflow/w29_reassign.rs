//! W29 改派同步错误任务 / 差异实体与处理人组织。

use erp_core::common::time::Instant;
use erp_identity::repository::OrganizationRepository;
use erp_integration::repository::IntegrationOpsExt;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::errors::Error;

/// 同步 W29 对象当前处理人及其有效内部组织。
///
/// # 参数
/// * `db` - 业务数据库
/// * `item` - 已改派的正式任务
/// * `target_user_id` - 接收人
/// * `executor` - 与任务写入相同的执行器
///
/// # 返回
/// 非 W29 对象时成功且不写入。
///
/// # 错误
/// 处理人缺少有效内部组织或对象不存在时拒绝。
pub(super) async fn reassign_handler(
    db: &Database,
    item: &mut WorkItem,
    target_user_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<()> {
    match item.business_object_type.as_str() {
        "integration_error_task" | "reconciliation_difference" => {},
        _ => return Ok(()),
    }
    let org = OrganizationRepository::new(db)
        .state(executor)
        .await
        .map_err(WorkflowError::from)?
        .own_org(target_user_id, Instant::now())
        .map_err(|error| map_service(Error::from(error)))?
        .filter(|org| !org.eq_ignore_ascii_case("company"))
        .ok_or_else(|| WorkflowError::ValidationError("处理人缺少有效内部组织".into()))?
        .to_string();
    item.owner_organization_id = org.clone();
    match item.business_object_type.as_str() {
        "integration_error_task" => {
            let mut task = db
                .integration_error_tasks()
                .find_work_item_integration_error_task(&item.business_object_id, executor)
                .await
                .map_err(WorkflowError::from)?
                .ok_or_else(|| WorkflowError::NotFound("集成异常任务不存在".into()))?;
            task.reassign_handler(target_user_id.to_string(), org)
                .map_err(|error| map_service(Error::from(error)))?;
            db.integration_error_tasks().update(&mut task, executor).await.map_err(WorkflowError::from)?;
        },
        "reconciliation_difference" => {
            let mut difference = db
                .reconciliation_differences()
                .find_work_item_reconciliation_difference(&item.business_object_id, executor)
                .await
                .map_err(WorkflowError::from)?
                .ok_or_else(|| WorkflowError::NotFound("对账差异不存在".into()))?;
            difference
                .reassign_handler(target_user_id.to_string(), org)
                .map_err(|error| map_service(Error::from(error)))?;
            db.reconciliation_differences()
                .update(&mut difference, executor)
                .await
                .map_err(WorkflowError::from)?;
        },
        _ => {},
    }
    Ok(())
}
