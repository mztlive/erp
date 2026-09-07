//! 采购变更命令的本域读取与原错误优先级。
use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseOrder, PurchaseOrderRevision};
use crate::repository::PurchaseOrderExt;
use crate::service::purchase_order::PurchaseOrderService;
use crate::{Error, Result};
use persistence_core::{Executor, NoTransaction};
impl PurchaseOrderService {
    /// 加载可发起变更的采购单及其当前生效版本。
    ///
    /// # 错误
    /// 采购单不存在、版本冲突或未生效时返回错误。
    pub async fn load_changeable_order(
        &self,
        id: &str,
        expected_lock_version: u64,
    ) -> Result<(PurchaseOrder, PurchaseOrderRevision)> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        order
            .ensure_expected_version(expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        let base_revision_id = order
            .revision_id_for_change()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        Ok((order, base_revision))
    }
    /// 同一采购单是否已有草稿或审批中的变更。
    ///
    /// # 错误
    /// 仓储失败或已存在进行中变更时返回错误。
    pub async fn ensure_no_in_progress_change(&self, purchase_order_id: &str) -> Result<()> {
        let has_in_progress = self
            .db
            .purchase_order()
            .has_in_progress_change(&purchase_order_id.to_string().into(), &mut NoTransaction)
            .await?;
        if has_in_progress {
            return Err(Error::ConflictError(
                "存在进行中的采购变更，不能重复发起".to_string(),
            ));
        }
        Ok(())
    }
    /// 锁定草稿变更单。
    ///
    /// # 错误
    /// 不存在、版本冲突或非草稿时返回错误。
    pub async fn lock_draft_change(
        &self,
        change_id: &str,
        expected_lock_version: u64,
    ) -> Result<PurchaseChangeOrder> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        change
            .ensure_expected_version(expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        change
            .ensure_draft_for_submission()
            .map_err(|_| Error::ConflictError("变更单已提交，请勿重复提交".to_string()))?;
        Ok(change)
    }

    /// 读取变更主表，保持调用方指定的事务执行器与缺失错误。
    pub async fn load_change(&self, id: &str, executor: &mut dyn Executor) -> Result<PurchaseChangeOrder> {
        self.db
            .purchase_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".into()))
    }
}
