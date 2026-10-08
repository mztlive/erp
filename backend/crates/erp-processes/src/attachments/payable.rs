//! 应付命令：在同一过账事务中登记银行回单文件。

use std::sync::Arc;

use application_core::AuditActor;
use erp_finance::dto::payable::CommitSupplierPaymentRequest;
use erp_identity::SharedRbacService;
use erp_support::{BankReceiptEvidencePolicy, PendingFileAssetRequest};
use erp_workflow::ApprovalObjectReadPort;
use mongodb::Database;

use super::pending::PendingFileAssets;
use crate::Result;
use crate::finance_posting::payable::{PayableService, SupplierPaymentWithAssetsResult};

/// 原子登记供应商付款与本次上传的银行回单。
///
/// # 参数
/// * `db` - 付款与附件使用的数据库
/// * `rbac` - 组合根共享授权读取器；付款 policy 事务实例保持局部装配
/// * `object_read` - 组合根装配的单据读取端口
/// * `req` - 付款事实、核销分配及原始幂等键
/// * `asset_requests` - 已上传、须随付款登记的银行回单元数据
/// * `actor` - 已通过鉴权的当前操作人
///
/// # 返回
/// 返回付款详情读取结果及本次附件提交状态；调用方必须先完成附件补偿，
/// 再展开详情读取结果。
///
/// # 错误
/// 回单政策、命令、授权或事务错误保持原分类。确认提交或幂等重放后的详情
/// 读取失败保存在 `view` 内，禁止丢弃附件提交状态后触发对象清理。
pub async fn commit_supplier_payment_with_assets(
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
    req: CommitSupplierPaymentRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierPaymentWithAssetsResult> {
    for request in &asset_requests {
        BankReceiptEvidencePolicy::validate(
            &request.registration.content_type,
            request.registration.sensitivity_class,
            request.registration.retention_class,
            false,
        )
        .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    }
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    PayableService::new(db)
        .with_rbac(rbac)
        .with_object_read(object_read)
        .commit_supplier_payment_with_assets(req, pending, &actor)
        .await
}
