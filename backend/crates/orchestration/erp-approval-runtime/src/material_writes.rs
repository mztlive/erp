//! 在审批提交事务装配材料来源授权，不信任既有附件关联提供转授权限。

use erp_identity::SharedRbacService;
use erp_read_models::workbench::freeze_approval_materials as freeze_materials;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::service::approval::copies::persist_copies;
use erp_workflow::service::approval::execution::PlannedWrites;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 同一提交执行器冻结材料，并重新证明原提交人具备材料转授资格。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `db` / `executor` - 业务提交数据库和事务。
/// * `snapshot` - 即将创建的不可变审批快照。
/// # 返回
/// 返回带授权后材料引用的快照。
/// # 错误
/// 账号、静态权限、资产归属或业务来源校验失败时中止提交。
pub async fn freeze_approval_materials<A: WorkflowAuthorizationPort>(
    identity_service: &SharedRbacService,
    db: &Database,
    snapshot: ApprovalSubjectSnapshot,
    auth: &A,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let mut snapshot = freeze_materials(identity_service, db, auth, snapshot, executor).await?;
    snapshot.freeze_resubmission_display()?;
    Ok(snapshot)
}

/// 在原提交事务保存冻结快照并生成开始抄送。
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
///
/// `db`、`snapshot`、`executor` 为原数据库、冻结快照和提交事务；
/// 身份资格在原事务执行器内重验。
/// # 返回
/// 无。
/// # 错误
/// 任一写入失败回滚提交。
pub async fn persist_approval_snapshot(
    identity_service: &SharedRbacService,
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    use erp_workflow::repository::{ApprovalIntegrationExt, ApprovalSubjectSnapshotRepositoryExt};
    use erp_workflow::service::approval::copies::persist_started_copy;
    db.approval_subject_snapshots().create_immutable_snapshot(snapshot, executor).await?;
    persist_started_copy(identity_service, db, snapshot, executor).await?;
    Ok(())
}

/// 撤回原事务内按绑定规则追加抄送。
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
///
/// `db`、`writes`、`executor` 为原数据库、取消计划及执行器；
/// 身份资格在原事务执行器内重验。
/// # 返回
/// 无。
/// # 错误
/// 任何持久化或资格读取错误中止撤回。
pub async fn persist_cancel_copies(
    identity_service: &SharedRbacService,
    db: &Database,
    writes: &PlannedWrites,
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_copies(identity_service, db, &writes.instance, &writes.events, executor).await?;
    Ok(())
}
