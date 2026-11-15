//! 跨域导入应用流程：批次应用、执行、确认与取代。

use std::sync::Arc;

use erp_identity::AuthorizationService;
use mongodb::Database;

mod batch;
mod command_event;
mod complete;
mod confirmation_query;
mod create_batch;
mod create_confirmation;
pub mod dto;
mod execution;
pub mod factories;
mod supersede;
mod work_items;
pub use work_items::ImportWorkItems;

#[cfg(test)]
mod confirmation_tests;
#[cfg(test)]
mod port_failure_tests;

const IMPORT_CONFIRMATION_OBJECT_TYPE: &str = "LEGACY_IMPORT_BATCH";
const IMPORT_CONFIRMATION_HANDLER: &str = "import_business_confirmation";
const IMPORT_CONFIRMATION_WORKSPACE: &str = "W18";
const IMPORT_CONFIRMATION_ORGANIZATION: &str = "company";
const IMPORT_CONFIRMATION_COMMAND_PREFIX: &str = "import-confirmation-command-";
const IMPORT_EXECUTION_COMMAND_PREFIX: &str = "import-execution-command-";

/// 跨域导入应用流程服务。
///
/// 持有应用、执行与确认命令的根事务。
#[derive(Clone)]
pub struct ImportApplyProcess {
    db: Database,
    authorization: Arc<AuthorizationService>,
    work_items: Arc<dyn ImportWorkItems>,
}

impl ImportApplyProcess {
    /// 创建绑定到 `db` 的导入应用流程。
    ///
    /// # 参数
    /// * `db` - 各领域仓储共用的 MongoDB 数据库。
    /// * `work_items` - 使用应用实时身份、事实和审计提供者的工作项授权能力。
    /// * `authorization` - 应用唯一共享授权服务。
    ///
    /// # 返回
    /// 返回流程服务。单条命令复用同一个执行器。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        work_items: Arc<dyn ImportWorkItems>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, work_items, authorization }
    }
}

/// 返回流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `import_apply`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "import_apply"
}

#[cfg(test)]
mod testing {
    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_workflow::entity::work_item::WorkItem;
    use erp_workflow::service::work_item::AuthorizedWorkItem;
    use mongodb::Client;
    use persistence_core::Executor;

    use super::*;

    struct UnreachableWorkItems;

    #[async_trait]
    impl ImportWorkItems for UnreachableWorkItems {
        async fn authorize_work_item(
            &self,
            _: &str,
            _: &AuditActor,
            _: &mut dyn Executor,
        ) -> erp_workflow::Result<AuthorizedWorkItem> {
            panic!("未授权不能读取工作项")
        }
        async fn ensure_domain_decision_access(
            &self,
            _: &AuditActor,
            _: &WorkItem,
            _: &mut dyn Executor,
        ) -> erp_workflow::Result<()> {
            panic!("未授权不能检查业务对象")
        }
        async fn require_assigned(
            &self,
            _: &AuditActor,
            _: &str,
            _: &mut dyn Executor,
        ) -> erp_workflow::Result<()> {
            panic!("未授权不能读取任务分配")
        }
    }

    pub(super) async fn fixture() -> (ImportApplyProcess, AuditActor) {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("import_authorization_unit");
        let service =
            ImportApplyProcess::new(db, Arc::new(UnreachableWorkItems), unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        (service, actor)
    }
}
