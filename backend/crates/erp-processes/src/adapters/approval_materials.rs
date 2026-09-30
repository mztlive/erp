//! 在审批提交事务装配材料来源授权，不信任既有附件关联提供转授权限。

use erp_read_models::workbench::freeze_approval_materials as freeze_materials;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use mongodb::Database;
use persistence_core::Executor;

use super::identity::shared_rbac_service;
use super::workflow::workflow_auth;
use crate::Result;

/// 同一提交执行器冻结材料，并重新证明原提交人具备材料转授资格。
///
/// # 参数
/// * `db` / `executor` - 业务提交数据库和事务。
/// * `snapshot` - 即将创建的不可变审批快照。
/// # 返回
/// 返回带授权后材料引用的快照。
/// # 错误
/// 账号、静态权限、资产归属或业务来源校验失败时中止提交。
pub(crate) async fn freeze_approval_materials(
    db: &Database,
    snapshot: ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let auth = workflow_auth(db.clone(), shared_rbac_service(db.clone()));
    Ok(freeze_materials(db, &auth, snapshot, executor).await?)
}
