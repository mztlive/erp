//! W29 本域加载与主题版本校验，不持有正式任务或权限。
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::integration_ops::{
    IntegrationErrorTask, ReconciliationDifference, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ResolutionVersionCheck,
};
use crate::repository::IntegrationOpsExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 为正式关联查询原始错误任务；此时不校验终态，保持原首错顺序。
///
/// # 参数
/// * `db` - 目标数据库
/// * `id` - 错误任务主键
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回错误任务，包括已终结任务。
///
/// # 错误
/// 记录不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
pub async fn load_error_task_for_association(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<IntegrationErrorTask> {
    load_error_task_record(db, id, executor).await
}

/// 加载错误任务并校验未终结（本域动作入口共用；终态与版本校验收敛于此）。
///
/// # 参数
/// * `db` - 目标数据库
/// * `id` - 错误任务主键
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回未终结的错误任务。
///
/// # 错误
/// 记录不存在时返回 `NotFound`；已解决或已关闭时返回 `ConflictError`；仓储读取失败时返回对应错误。
pub(super) async fn load_error_task(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<IntegrationErrorTask> {
    let task = load_error_task_record(db, id, executor).await?;
    if task.is_terminal() {
        return Err(Error::ConflictError("集成错误任务已终结".to_string()));
    }
    Ok(task)
}

/// 按主键读取错误任务记录（加载层统一入口；终态与版本校验由调用方装配）。
///
/// # 参数
/// * `db` - 数据库句柄
/// * `id` - 错误任务主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回错误任务记录。
///
/// # 错误
/// 记录不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
async fn load_error_task_record(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<IntegrationErrorTask> {
    db.integration_error_tasks()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("集成错误任务不存在".to_string()))
}

/// 校验错误任务业务主题版本仍与命令一致。
///
/// # 参数
/// * `task` - 已加载的错误任务
/// * `expected` - 命令中的业务主题版本
///
/// # 返回
/// 去除首尾空白后与当前乐观锁版本一致时成功。
///
/// # 错误
/// 不一致时返回 `ConflictError`。
pub(super) fn ensure_error_task_subject(task: &IntegrationErrorTask, expected: &str) -> Result<()> {
    if !task.has_subject_version(expected) {
        return Err(Error::ConflictError("错误任务业务版本已变化".to_string()));
    }
    Ok(())
}

/// 按主键读取对账差异记录（加载层统一入口；终态与版本校验由调用方装配）。
///
/// # 参数
/// * `db` - 数据库句柄
/// * `id` - 对账差异主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回对账差异记录。
///
/// # 错误
/// 记录不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
pub(super) async fn load_difference(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<ReconciliationDifference> {
    db.reconciliation_differences()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("对账差异不存在".to_string()))
}

/// 校验差异业务主题版本与最新决定序号一致。
///
/// # 参数
/// * `latest` - 最新决定；`None` 表示尚无决定
/// * `expected` - 命令中的业务主题版本
///
/// # 返回
/// 领域版本检查为当前时成功。
///
/// # 错误
/// 版本非法或已变化时返回 `ConflictError`。
pub(super) fn ensure_difference_subject(
    latest: Option<&ReconciliationDifferenceResolution>,
    expected: &str,
) -> Result<()> {
    match ReconciliationDifferenceResolution::check_version(expected, latest) {
        ResolutionVersionCheck::Current => Ok(()),
        ResolutionVersionCheck::Invalid | ResolutionVersionCheck::Stale => {
            Err(Error::ConflictError("对账差异业务版本已变化".to_string()))
        },
    }
}

/// 读取差异的最新决定记录。
///
/// # 参数
/// * `db` - 目标数据库
/// * `id` - 对账差异主键
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回最新决定；尚无处理记录时返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回对应错误。
pub(super) async fn latest_resolution(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ReconciliationDifferenceResolution>> {
    db.reconciliation_difference_resolutions()
        .find_latest_by_difference(&ReconciliationDifferenceId::new(id.to_string()), executor)
        .await
        .map_err(Into::into)
}

/// 拒绝已形成正式结论的差异。
///
/// # 参数
/// * `latest` - 最新决定；`None` 视为仍开放
///
/// # 返回
/// 尚无决定或最新决定非终态时成功。
///
/// # 错误
/// 已形成正式结论时返回 `ConflictError`。
pub(super) fn ensure_difference_open(latest: Option<&ReconciliationDifferenceResolution>) -> Result<()> {
    if !ReconciliationDifferenceResolution::is_open(latest) {
        return Err(Error::ConflictError("对账差异已形成正式结论".to_string()));
    }
    Ok(())
}
