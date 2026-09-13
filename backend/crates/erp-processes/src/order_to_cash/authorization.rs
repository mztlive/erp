//! 销售命令在原业务事务内重验资源动作、责任和业务版本。

use application_core::AuditActor;
use erp_identity::Permission;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::{entity::sales_order::SalesOrder, repository::SalesOrderExt};
use persistence_core::Executor;

use super::SalesOrderCommandProcess;
use crate::{Error, Result};

/// 命令上下文只保存请求身份；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct SalesCommandAccess {
    db: mongodb::Database,
    access: SalesAccess,
    actor: AuditActor,
    action: &'static str,
    permissions: Vec<Permission>,
}

impl SalesOrderCommandProcess {
    /// 构造写命令检查器；缺少身份装配时拒绝，不退回路由级授权。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
    ) -> Result<SalesCommandAccess> {
        Ok(SalesCommandAccess {
            db: self.db.clone(),
            access: SalesAccess::new(self.db.clone(), self.require_rbac()?.clone()),
            actor: actor.clone(),
            action,
            permissions: vec![],
        })
    }
}

impl SalesCommandAccess {
    /// 创建并提交必须由同一角色同时提供两个动作。
    pub(super) fn require_submit(mut self, submit: bool) -> Result<Self> {
        if submit {
            self.permissions.push(Permission::parse("sales_order:submit")?);
        }
        Ok(self)
    }

    /// 创建并提交的两个资源动作范围分别计算后求交，不能仅证明动作而复用创建范围。
    async fn scope(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<erp_sales::repository::sales_order::scope::SalesReadScope> {
        let (_, mut scope) = self
            .access
            .resolve(&self.actor, self.action, &self.permissions, executor)
            .await?;
        if !self.permissions.is_empty() {
            let (_, submit) = self
                .access
                .resolve(
                    &self.actor,
                    "submit",
                    &[Permission::parse("sales_order:create")?],
                    executor,
                )
                .await?;
            scope.required_scopes.push(submit);
        }
        Ok(scope)
    }

    /// 在调用方事务内读取当前可操作单据；历史参与不会产生写资格。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesOrder> {
        let scope = self.scope(executor).await?;
        self.db
            .sales_orders()
            .find_authorized(id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或无权操作".into()))
    }

    /// 写入前重新读取当前责任与版本，防止预读取后交接或状态变化。
    pub(super) async fn revalidate(
        &self,
        id: &str,
        expected: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.current(id, executor).await?;
        if current.base.version != expected {
            return Err(Error::ConflictError(
                "销售单责任或版本已变化，请刷新后重试".into(),
            ));
        }
        Ok(())
    }

    /// 新单使用即将持久化的显式责任解释创建范围，不能由创建人审计字段兜底。
    pub(super) async fn creation(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        let scope = self.scope(executor).await?;
        if !scope.allows_creation(
            &order.sales_owner_user_id,
            &order.business_org_unit_id,
            order.customer_id.as_ref(),
        ) {
            return Err(Error::Forbidden("没有该业务责任范围的销售建单权限".into()));
        }
        Ok(())
    }
}
