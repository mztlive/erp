//! 跨域导入应用流程：批次应用、执行、确认与取代。

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
pub struct ImportApplyService {
    db: Database,
}

impl ImportApplyService {
    /// 创建绑定到 `db` 的导入应用流程。
    ///
    /// # 参数
    /// * `db` - 各领域仓储共用的 MongoDB 数据库。
    ///
    /// # 返回
    /// 返回流程服务。单条命令复用同一个执行器。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
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
