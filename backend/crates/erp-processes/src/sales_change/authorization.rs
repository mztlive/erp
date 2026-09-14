//! 销售变更命令沿原销售单动作在原事务内重验对象范围。

use application_core::AuditActor;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use persistence_core::Executor;

use super::SalesChangeProcess;
use crate::Result;

/// 命令上下文只保存请求身份；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct SalesChangeCommandAccess {
    /// 销售范围解析器。
    access: SalesAccess,
    /// 已认证操作人。
    actor: AuditActor,
    /// 本次命令对应的来源销售单动作。
    action: &'static str,
}

impl SalesChangeProcess {
    /// 构造写命令范围检查器；缺少身份装配时拒绝，不退回路由级授权。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的销售单动作
    ///
    /// # 返回
    /// 返回可在事务内重验来源销售单范围的检查器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 每次检查均重新解析服务端授权，不得复用列表上下文。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
    ) -> Result<SalesChangeCommandAccess> {
        Ok(SalesChangeCommandAccess {
            access: SalesAccess::new(self.db.clone(), self.require_rbac()?),
            actor: actor.clone(),
            action,
        })
    }
}

impl SalesChangeCommandAccess {
    /// 在调用方事务内读取当前可操作的来源销售单；历史参与不会产生写资格。
    ///
    /// # 参数
    /// * `id` - 来源销售单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回销售单。
    ///
    /// # 错误
    /// 不存在或越权时返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原事务重验，不得把入口事前检查当作凭证。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesOrder> {
        self.access
            .require_object(&self.actor, self.action, id, &[], executor)
            .await
            .map_err(crate::Error::from)
    }
}
