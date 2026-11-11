//! 开票申请命令和历史回放的当前资格；读取共享调用方事务，不以旧绑定授予权限。
use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission};
use erp_read_models::finance::receivable::ReceivableReadService;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, FinancePostingPorts, Result};

/// 申请人的正式业务动作；保存沿用提交资格。
#[derive(Clone, Copy)]
pub(super) enum RequestAction {
    Submit,
    Cancel,
}
impl RequestAction {
    /// 从持久化命令目录识别动作，未知目录失败关闭。
    pub(super) fn from_command(command: &CommandReceipt) -> Result<Self> {
        match command.action() {
            "sales_invoice_request.submit" => Ok(Self::Submit),
            "sales_invoice_request.cancel" => Ok(Self::Cancel),
            _ => Err(Error::Internal("开票申请命令动作未登记".into())),
        }
    }
    fn permission(self) -> &'static str {
        match self {
            Self::Submit => "sales_invoice_request:submit",
            Self::Cancel => "sales_invoice_request:cancel",
        }
    }
}

/// 只读资格与回放端口；没有保存或启动能力，回放无法产生额外业务写入。
#[async_trait]
trait RequestAccess: Send + Sync {
    type View: Send;
    async fn allowed(
        &self,
        actor: &AuditActor,
        action: RequestAction,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn readable(
        &self,
        actor: &AuditActor,
        id: &str,
        action: RequestAction,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn committed(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>>;
    async fn view(&self, id: &str, executor: &mut dyn Executor) -> Result<Self::View>;
}

pub(super) struct MongoAccess<'a> {
    pub db: &'a Database,
    pub rbac: &'a FinancePostingPorts,
}
#[async_trait]
impl RequestAccess for MongoAccess<'_> {
    type View = InvoiceRequestView;
    async fn allowed(
        &self,
        actor: &AuditActor,
        action: RequestAction,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let service = AuthorizationService::new(
            self.db.clone(),
            self.rbac.identity.clone(),
            AuthorizationRegistry::default(),
        );
        Ok(service.has_permission(actor, &Permission::parse(action.permission())?, executor).await?)
    }
    async fn readable(
        &self,
        actor: &AuditActor,
        id: &str,
        action: RequestAction,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let request = super::load(self.db, id, executor).await?;
        match self
            .rbac
            .funds
            .require_sales_sources(
                actor,
                action.permission(),
                &[request.sales_order_id.to_string()],
                executor,
            )
            .await
            .map_err(Error::from)
        {
            Ok(()) => Ok(true),
            Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }
    async fn committed(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(command, executor)
            .await?)
    }
    async fn view(&self, id: &str, executor: &mut dyn Executor) -> Result<Self::View> {
        Ok(ReceivableReadService::new(self.db.clone())
            .invoice_request_detail_with_executor(id, executor)
            .await?)
    }
}

/// 当前账号及动作资格先于命令回执和业务对象读取。
async fn actor_access<P: RequestAccess>(
    port: &P,
    actor: &AuditActor,
    action: RequestAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !port.allowed(actor, action, executor).await? {
        return Err(Error::Forbidden("当前账号不可办理该开票申请".into()));
    }
    Ok(())
}
/// 当前完整来源范围由统一授权入口检查，不依赖单据可编辑状态。
async fn object_access<P: RequestAccess>(
    port: &P,
    actor: &AuditActor,
    id: &str,
    action: RequestAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !port.readable(actor, id, action, executor).await? {
        return Err(Error::Forbidden("当前账号无权读取开票申请的来源销售单".into()));
    }
    Ok(())
}
/// 写事务在业务状态、额度或绑定修改之前重验当前资格。
pub(super) async fn authorize(
    port: &MongoAccess<'_>,
    actor: &AuditActor,
    id: &str,
    action: RequestAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize_request(port, actor, id, action, executor).await
}
/// 所有写入和已完成结果沿同一资格顺序，不根据旧绑定或单据状态短路。
async fn authorize_request<P: RequestAccess>(
    port: &P,
    actor: &AuditActor,
    id: &str,
    action: RequestAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    actor_access(port, actor, action, executor).await?;
    object_access(port, actor, id, action, executor).await
}
/// 命令回放和未知结果恢复共用只读分支，旧回执不得替代当前对象资格。
async fn replay<P: RequestAccess>(
    port: &P,
    actor: &AuditActor,
    command: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<P::View>> {
    actor_access(port, actor, RequestAction::from_command(command)?, executor).await?;
    let Some(id) = port.committed(command, executor).await? else { return Ok(None) };
    let result = async {
        object_access(port, actor, &id, RequestAction::from_command(command)?, executor).await?;
        port.view(&id, executor).await.map(Some)
    }
    .await;
    super::command::committed_result(result)
}

impl MongoAccess<'_> {
    /// 在原事务中校验完整回执并读取同快照授权视图。
    pub(super) async fn replay(
        &self,
        actor: &AuditActor,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<InvoiceRequestView>> {
        replay(self, actor, command, executor).await
    }
    /// 当前命令完成后重新确认操作资格及来源范围，再读取同事务详情。
    pub(super) async fn result(
        &self,
        actor: &AuditActor,
        id: &str,
        action: RequestAction,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestView> {
        authorize(self, actor, id, action, executor).await?;
        self.view(id, executor).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use erp_core::AccountKind;
    use erp_finance::FinanceCommandReceipt;
    use persistence_core::NoTransaction;

    use super::*;

    struct RecordingAccess {
        active: AtomicBool,
        allowed: AtomicBool,
        readable: AtomicBool,
        receipt: Option<FinanceCommandReceipt>,
        calls: Mutex<Vec<&'static str>>,
    }
    impl RecordingAccess {
        fn new(with_receipt: bool) -> Self {
            Self {
                active: AtomicBool::new(true),
                allowed: AtomicBool::new(true),
                readable: AtomicBool::new(true),
                receipt: with_receipt.then(|| {
                    FinanceCommandReceipt::resource(&command(1), "request".into(), "audit".into()).unwrap()
                }),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn record(&self, call: &'static str) {
            self.calls.lock().unwrap().push(call);
        }
        fn calls(&self) -> Vec<&'static str> {
            std::mem::take(&mut *self.calls.lock().unwrap())
        }
    }
    fn actor() -> AuditActor {
        AuditActor::new("owner".into(), "owner".into(), AccountKind::Admin)
    }
    fn command(value: u64) -> CommandReceipt {
        CommandReceipt::from_payload(
            "invoice-request-submit-",
            "owner",
            "sales_invoice_request.submit",
            "sales_invoice_request",
            "key",
            &value,
        )
        .unwrap()
    }
    #[async_trait]
    impl RequestAccess for RecordingAccess {
        type View = String;
        async fn allowed(&self, _: &AuditActor, _: RequestAction, _: &mut dyn Executor) -> Result<bool> {
            self.record("action");
            Ok(self.active.load(Ordering::SeqCst) && self.allowed.load(Ordering::SeqCst))
        }
        async fn readable(
            &self,
            _: &AuditActor,
            id: &str,
            _: RequestAction,
            _: &mut dyn Executor,
        ) -> Result<bool> {
            self.record("scope");
            assert_eq!(id, "request");
            Ok(self.readable.load(Ordering::SeqCst))
        }
        async fn committed(&self, cmd: &CommandReceipt, _: &mut dyn Executor) -> Result<Option<String>> {
            self.record("receipt");
            self.receipt.as_ref().map(|r| r.resource_id(cmd).map_err(Error::from)).transpose()
        }
        async fn view(&self, id: &str, _: &mut dyn Executor) -> Result<String> {
            self.record("view");
            Ok(id.into())
        }
    }

    #[tokio::test]
    async fn successful_replay_is_read_only_and_rechecks_revoked_action_or_scope() {
        let port = RecordingAccess::new(true);
        assert_eq!(
            replay(&port, &actor(), &command(1), &mut NoTransaction).await.unwrap(),
            Some("request".into())
        );
        assert_eq!(port.calls(), vec!["action", "receipt", "scope", "view"]);
        port.allowed.store(false, Ordering::SeqCst);
        assert!(replay(&port, &actor(), &command(1), &mut NoTransaction).await.is_err());
        assert_eq!(port.calls(), vec!["action"]);
        port.allowed.store(true, Ordering::SeqCst);
        port.readable.store(false, Ordering::SeqCst);
        assert!(matches!(
            replay(&port, &actor(), &command(1), &mut NoTransaction).await,
            Err(Error::CommittedResultUnavailable(_))
        ));
        assert_eq!(port.calls(), vec!["action", "receipt", "scope"]);
    }

    #[tokio::test]
    async fn replay_rejects_inactive_actor_and_changed_payload() {
        let port = RecordingAccess::new(true);
        port.active.store(false, Ordering::SeqCst);
        assert!(replay(&port, &actor(), &command(1), &mut NoTransaction).await.is_err());
        assert_eq!(port.calls(), vec!["action"]);
        port.active.store(true, Ordering::SeqCst);
        assert!(replay(&port, &actor(), &command(2), &mut NoTransaction).await.is_err());
        assert_eq!(port.calls(), vec!["action", "receipt"]);
    }

    #[tokio::test]
    async fn new_submission_and_existing_binding_require_the_same_current_source_qualification() {
        let port = RecordingAccess::new(false);
        assert!(replay(&port, &actor(), &command(1), &mut NoTransaction).await.unwrap().is_none());
        assert_eq!(port.calls(), vec!["action", "receipt"]);
        authorize_request(&port, &actor(), "request", RequestAction::Submit, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(port.calls(), vec!["action", "scope"]);
        port.readable.store(false, Ordering::SeqCst);
        assert!(
            authorize_request(&port, &actor(), "request", RequestAction::Submit, &mut NoTransaction)
                .await
                .is_err()
        );
        assert_eq!(port.calls(), vec!["action", "scope"]);
    }

    #[tokio::test]
    async fn authorized_delegate_can_handle_the_request_but_source_denial_still_stops_access() {
        let port = RecordingAccess::new(false);
        let delegate = AuditActor::new("delegate".into(), "delegate".into(), AccountKind::Admin);
        for action in [RequestAction::Submit, RequestAction::Cancel] {
            authorize_request(&port, &delegate, "request", action, &mut NoTransaction).await.unwrap();
            assert_eq!(port.calls(), vec!["action", "scope"]);
        }
        port.allowed.store(false, Ordering::SeqCst);
        assert!(matches!(
            authorize_request(&port, &delegate, "request", RequestAction::Cancel, &mut NoTransaction).await,
            Err(Error::Forbidden(message)) if message == "当前账号不可办理该开票申请"
        ));
        assert_eq!(port.calls(), vec!["action"]);
        port.allowed.store(true, Ordering::SeqCst);
        port.readable.store(false, Ordering::SeqCst);
        assert!(matches!(
            authorize_request(&port, &delegate, "request", RequestAction::Submit, &mut NoTransaction).await,
            Err(Error::Forbidden(message)) if message == "当前账号无权读取开票申请的来源销售单"
        ));
        assert_eq!(port.calls(), vec!["action", "scope"]);
    }
}
