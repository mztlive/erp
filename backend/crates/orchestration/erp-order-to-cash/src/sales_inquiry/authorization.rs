//! 询价用例调用固定授权入口；资源实现由 server 一次绑定。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService, CompiledAccess};
use erp_permissions::{Permission, internal, sales_inquiry};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_workflow::entity::work_item::WorkItem;
use persistence_core::Executor;

use crate::repository::sales_inquiry_access::inquiry_candidate;
use crate::{Error, Result};

// 检查询价的当前对象或创建候选。
//
// authorization 为共享服务，actor/permission 为入口身份和动作，inquiry 为业务事实，executor 为原事务。
//
// 固定权限和对象检查通过时成功。缺少资格、范围、事实或执行边界时拒绝。
pub(super) async fn require(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    permission: &Permission,
    inquiry: &SalesInquiry,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_sales_permission(permission)?;
    let target = if permission == &sales_inquiry::COMMANDS_SALES_CREATE
        || permission == &internal::SALES_INQUIRY_IMPORT_CREATE
    {
        AccessTarget::Candidate(inquiry_candidate(inquiry))
    } else {
        AccessTarget::record(&inquiry.base.id)?
    };
    authorization.require(actor, permission, target, executor).await?;
    Ok(())
}

// 编译销售询价固定查询。
//
// authorization 为共享服务，actor/permission 为当前查询入口，executor 为原读取阶段。查询凭证和原共享服务。
//
// 缺少资格、范围、事实或执行边界时拒绝。
pub(super) async fn query(
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<(Arc<CompiledAccess>, Arc<AuthorizationService>)> {
    ensure_sales_permission(permission)?;
    let compiled = authorization.query(actor, permission, executor).await?;
    Ok((Arc::new(compiled), authorization.clone()))
}

// 检查精确采购任务及版本。
//
// authorization 为共享服务，actor/permission 为当前入口，task 为读取的精确任务，executor 为原事务。
//
// 任务、版本和操作关系均符合时成功。缺少资格、范围、事实或执行边界时拒绝。
pub(super) async fn require_task_owner(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    permission: &Permission,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    if permission.resource() != sales_inquiry::QUERY_PROCUREMENT_LIST.resource() {
        return Err(Error::ValidationError("采购询价任务不能使用其他资源权限".into()));
    }
    authorization
        .require(
            actor,
            permission,
            AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
            executor,
        )
        .await?;
    Ok(())
}

// 销售读取器只接受销售询价根，禁止用其他资源的同名记录放行。
fn ensure_sales_permission(permission: &Permission) -> Result<()> {
    if permission.resource() != sales_inquiry::QUERY_SALES_LIST.resource() {
        return Err(Error::ValidationError("销售询价访问器收到其他资源权限".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sales_inquiry_bridge_keeps_procurement_permissions_on_the_task_path() {
        assert!(ensure_sales_permission(&sales_inquiry::COMMANDS_SALES_SAVE).is_ok());
        assert!(ensure_sales_permission(&internal::SALES_INQUIRY_IMPORT_CREATE).is_ok());
        assert!(matches!(
            ensure_sales_permission(&sales_inquiry::QUERY_PROCUREMENT_DETAIL),
            Err(Error::ValidationError(_))
        ));
    }
}
