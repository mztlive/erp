//! 订单与供应商申请任务的独立对象授权；读模型与任务命令复用同一合同。

use application_core::AuditActor;
use persistence_core::Executor;

use crate::error::{Error, Result};
use crate::ports::{ObjectFact, ObjectKind, OrderTaskSource, TaskReadAction, WorkflowAuthorizationPort};

/// 重验供应商申请任务的内部账号及独立对象读取范围。
///
/// # 参数
/// * `auth` - 已装配的工作流授权 Port。
/// * `actor` - 同一执行器内已经验证的操作人或候选人身份。
/// * `kind` - 固定任务关系中的对象种类。
/// * `object_id` - 权威对象主键，不从展示字段推断。
/// * `executor` - 调用方事务或查询执行器。
/// # 返回
/// 供应商申请详情范围满足时返回成功；其他任务沿各自授权政策继续执行。
/// # 错误
/// 内部身份失效、范围越界或适配器未装配时失败关闭。
pub async fn require_supplier_portal_task_read(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    kind: ObjectKind,
    object_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if kind != ObjectKind::SupplierPortalRequest {
        return Ok(());
    }
    if auth.supplier_portal_request_readable(actor, object_id, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("当前账号不具备供应商申请读取范围".into()))
}

/// 重验当前内部账号对精确供应商申请的商品与供给业务确认资格。
///
/// # 参数
/// * `auth` - 已装配的工作流授权 Port
/// * `actor` - 同一执行器内已经验证的内部确认人身份
/// * `kind` - 固定任务关系中的对象种类
/// * `object_id` - 权威申请主键
/// * `executor` - 调用方执行器
///
/// # 返回
/// 供应商申请业务动作和责任范围全部满足时成功；其他对象沿原授权规则执行。
/// # 错误
/// 外部身份、资格撤销或越界拒绝；配置和基础设施错误原样传播。
pub(super) async fn require_supplier_portal_task_review(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    kind: ObjectKind,
    object_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if kind != ObjectKind::SupplierPortalRequest {
        return Ok(());
    }
    if auth.supplier_portal_request_reviewable(actor, object_id, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("当前账号不具备供应商申请所需的商品或供给业务确认资格".into()))
}

/// 隐藏不可见对象的存在性，保留配置、版本及基础设施错误供调用方处理。
///
/// # 参数
/// * `error` - 对象读取或参与校验返回的错误
///
/// # 返回
/// 读取拒绝统一为 Forbidden，其他错误原样返回，不得伪装成候选不合格。
///
/// # 错误
/// 不返回错误。
pub(super) fn task_read_error(error: Error) -> Error {
    match error {
        Error::Forbidden(_) | Error::NotFound(_) => {
            Error::Forbidden("当前账号不具备任务业务对象读取资格".into())
        },
        error => error,
    }
}

/// 按权威订单来源重验当前账号的业务对象读取范围。
///
/// # 参数
/// * `auth` - 已装配的工作流授权 Port。
/// * `actor` - 同一执行器内已经验证的操作人或候选接收人身份。
/// * `kind` - 任务注册表固定的对象种类。
/// * `fact` - 同一执行器读取的业务对象事实。
/// * `action` - 原用例要求的任务读取入口。
/// * `executor` - 调用方查询或任务事务执行器。
/// # 返回
/// 订单来源通过原入口的独立授权时成功；其他对象仍执行其所属阶段的授权。
/// # 错误
/// 必需来源缺失、账号失效、未装配或详情范围越界时拒绝。
pub async fn require_order_task_read(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    kind: ObjectKind,
    fact: &ObjectFact,
    action: TaskReadAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !OrderTaskSource::required_for(kind) {
        return Ok(());
    }
    let source = fact
        .order_scope_source
        .as_ref()
        .filter(|source| source.matches_kind(kind))
        .ok_or_else(|| Error::Internal("订单关联任务缺少权威范围来源".into()))?;

    auth.require_order_task_read(actor, source, action, executor).await
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::ports::FailClosedWorkflowAuthorizationPort;

    #[derive(Clone, Default)]
    struct SourceAuthorization {
        calls: Arc<Mutex<Vec<(String, OrderTaskSource, TaskReadAction)>>>,
    }

    impl WorkflowAuthorizationPort for SourceAuthorization {
        async fn require_order_task_read(
            &self,
            actor: &AuditActor,
            source: &OrderTaskSource,
            action: TaskReadAction,
            _: &mut dyn Executor,
        ) -> Result<()> {
            self.calls.lock().unwrap().push((actor.id().into(), source.clone(), action));
            match source {
                OrderTaskSource::Sales(id) if id == "denied" => Err(Error::Forbidden("revoked".into())),
                OrderTaskSource::Sales(id) if id == "changed" => {
                    Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()))
                },
                _ => Ok(()),
            }
        }
    }

    #[tokio::test]
    async fn exact_source_check_preserves_entry_and_never_uses_display_root() {
        let auth = SourceAuthorization::default();
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        let source = OrderTaskSource::Purchase("purchase".into());
        let fact = ObjectFact::new("display-root", "title", "creator").with_order_source(source.clone());
        for (action, permission) in
            [(TaskReadAction::List, "work_item:list"), (TaskReadAction::Detail, "work_item:detail")]
        {
            assert_eq!(action.permission(), permission);
            require_order_task_read(
                &auth,
                &actor,
                ObjectKind::PurchaseReceipt,
                &fact,
                action,
                &mut NoTransaction,
            )
            .await
            .unwrap();
        }
        assert_eq!(
            auth.calls.lock().unwrap().as_slice(),
            &[
                ("actor".into(), source.clone(), TaskReadAction::List),
                ("actor".into(), source, TaskReadAction::Detail),
            ]
        );
    }

    #[tokio::test]
    async fn missing_or_mismatched_source_fails_before_authorization() {
        let auth = SourceAuthorization::default();
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        for fact in [
            ObjectFact::new("display-root", "title", "creator"),
            ObjectFact::new("display-root", "title", "creator")
                .with_order_source(OrderTaskSource::Purchase("wrong".into())),
        ] {
            let result = require_order_task_read(
                &auth,
                &actor,
                ObjectKind::Delivery,
                &fact,
                TaskReadAction::List,
                &mut NoTransaction,
            )
            .await;
            assert!(matches!(result, Err(Error::Internal(_))));
        }
        assert!(auth.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn exact_source_check_preserves_denial_and_operational_failures() {
        let auth = SourceAuthorization::default();
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        for action in [TaskReadAction::List, TaskReadAction::Detail] {
            let denied = ObjectFact::new("display", "title", "creator")
                .with_order_source(OrderTaskSource::Sales("denied".into()));
            assert!(
                matches!(require_order_task_read(&auth, &actor, ObjectKind::Delivery, &denied, action, &mut NoTransaction).await, Err(Error::Forbidden(message)) if message == "revoked")
            );
            let changed = ObjectFact::new("display", "title", "creator")
                .with_order_source(OrderTaskSource::Sales("changed".into()));
            assert!(
                matches!(require_order_task_read(&auth, &actor, ObjectKind::Delivery, &changed, action, &mut NoTransaction).await, Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED")
            );
        }
    }

    #[tokio::test]
    async fn non_order_tasks_skip_order_authorization_but_unwired_order_and_portal_fail_closed() {
        let auth = FailClosedWorkflowAuthorizationPort;
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        let fact = ObjectFact::new("display", "title", "creator");
        require_order_task_read(
            &auth,
            &actor,
            ObjectKind::SupplierPortalRequest,
            &fact,
            TaskReadAction::List,
            &mut NoTransaction,
        )
        .await
        .unwrap();
        let source = OrderTaskSource::Sales("sales".into());
        for action in [TaskReadAction::List, TaskReadAction::Detail] {
            assert!(auth.require_order_task_read(&actor, &source, action, &mut NoTransaction).await.is_err());
        }
        assert!(
            require_supplier_portal_task_read(
                &auth,
                &actor,
                ObjectKind::SupplierPortalRequest,
                "request",
                &mut NoTransaction
            )
            .await
            .is_err()
        );
    }
}
