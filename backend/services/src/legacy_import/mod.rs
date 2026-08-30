//! 域 D22 `legacy_import` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建批次（批次 + 来源行 + D04 后台任务 + 审计日志）→ 跨集合，
//!   `database::Transactional::with_transaction` 内经 `LegacyImportRepository::create_batch_with_rows`
//!   与 D04/D02 仓储写入，保证「批次 + 明细 + 后台任务」原子可见；
//! - 创建确认任务（确认 + `work_item` + 批次摘要 + 审计日志）→ 跨集合事务；
//! - 完成确认（确认决策 + `workflow_action` + 批次 + 任务终态 + 稳定收据）
//!   → `CompleteImportBusinessConfirmation` 唯一强类型事务；
//! - 批次应用（行状态推进 + 批次统计 + 后台任务进度 + 审计日志）→ 跨集合事务；
//! - 其余单集合查询传 `&mut NoTransaction`。
//!
//! 跨域协作只经 `DatabaseExt` 调对方 Repository（P3-service-api §2）：
//! - D04 `bulk_job`：登记/推进 `background_job`（批次导入的后台任务）；
//! - D05 `file_asset`：批次创建时校验资产引用存在；
//! - D07 `party`：客户行导入前校验目标主体存在（`CUSTOMER_NOT_FOUND`）。
//!
//! 幂等约定：批次号、`(batch_id, scope, trial_version)` 与不可逆审计收据
//! 是权威去重依据；重复创建或完成只在全部锁定字段一致时返回原结果。

use database::LegacyImportExt;
use mongodb::Database;

mod batch;
mod confirmation;
pub mod dto;
mod execution;
mod query;
mod receipt;

pub use self::dto::{
    ApplyLegacyImportBatchRequest, ApplyRowResult, CompleteImportBusinessConfirmationCommand,
    CompleteImportBusinessConfirmationResult, CreateLegacyImportBatchRequest,
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, ImportBusinessConfirmationWorkItemView, ImportExecutionAction,
    ImportExecutionCommand, ImportExecutionNextStep, ImportExecutionResult, ImportExecutionResultStatus,
    ImportRowRequest, LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView,
    LegacyImportConfirmationListParams, LegacyImportConfirmationView, LegacyImportRowListParams,
    LegacyImportRowView, PageView,
};

const IMPORT_CONFIRMATION_OBJECT_TYPE: &str = "LEGACY_IMPORT_BATCH";
const IMPORT_CONFIRMATION_HANDLER: &str = "import_business_confirmation";
const IMPORT_CONFIRMATION_WORKSPACE: &str = "W18";
const IMPORT_CONFIRMATION_ORGANIZATION: &str = "company";
const IMPORT_CONFIRMATION_AUDIT_PREFIX: &str = "import-confirmation-command-";
const IMPORT_EXECUTION_AUDIT_PREFIX: &str = "import-execution-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// 导入批次列表筛选条件类型（经 `LegacyImportExt` 关联类型跨 crate 可达）。
type LegacyImportBatchFilter = <mongodb::Database as LegacyImportExt>::LegacyImportBatchFilter;
/// 导入行列表筛选条件类型。
type LegacyImportRowFilter = <mongodb::Database as LegacyImportExt>::LegacyImportRowFilter;
/// 导入确认列表筛选条件类型。
type LegacyImportConfirmationFilter = <mongodb::Database as LegacyImportExt>::LegacyImportConfirmationFilter;

/// 旧数据导入服务。
///
/// 提供导入批次、导入行与业务确认事实的创建、查询与状态推进编排。
pub struct LegacyImportService {
    db: Database,
}

impl LegacyImportService {
    /// 创建旧数据导入服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
