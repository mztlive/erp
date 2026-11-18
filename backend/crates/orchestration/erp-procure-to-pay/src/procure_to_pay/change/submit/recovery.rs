//! 采购变更完整提交命令的只读回放；首次结果不随当前审批状态变化。

use erp_procurement::dto::purchase_order::PurchaseChangeSubmitResult;
use erp_procurement::entity::purchase_order::{PurchaseCommandReceipt, PurchaseCommandReceiptIdentity};
use erp_procurement::repository::{PurchaseCommandExt, PurchaseOrderExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 已授权后按完整原请求回读不可变回执，拒绝同键不同版本或目标内容。
pub(in crate::procure_to_pay) async fn replay_change_command(
    db: &Database,
    identity: &PurchaseCommandReceiptIdentity,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseChangeSubmitResult>> {
    let Some(record) = db
        .purchase_command_receipts::<PurchaseChangeSubmitResult>()
        .find_by_id_including_deleted(identity.receipt_id(), executor)
        .await?
    else {
        return Ok(None);
    };
    let receipt = PurchaseCommandReceipt::decode(record, identity, fingerprint)
        .map_err(|_| Error::ConflictError("无法将本次提交与原采购变更记录对应，请先核对审批状态".into()))?
        .into_payload();

    let submission =
        db.purchase_change_submissions().find_by_id(&receipt.submission_id, executor).await?.ok_or_else(
            || Error::ConflictError("找不到采购变更原先提交审批的单据内容，请联系管理员核对".into()),
        )?;
    if submission.purchase_change_order_id.as_ref() != receipt.change_id
        || submission.submission_no != receipt.submission_no
    {
        return Err(Error::ConflictError("采购变更的原提交记录与送审单据内容不符，请联系管理员核对".into()));
    }
    Ok(Some(receipt))
}
