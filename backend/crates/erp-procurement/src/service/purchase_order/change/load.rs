//! 采购变更命令的本域读取；来源采购单范围必须由调用方在本模块校验之前证明。
use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseOrder, PurchaseOrderRevision};
use crate::repository::PurchaseOrderExt;
use crate::service::purchase_order::PurchaseOrderService;
use crate::{Error, Result};
use persistence_core::{Executor, NoTransaction};

impl PurchaseOrderService {
    /// 在来源采购单已按写动作证明可见后，加载可发起变更的当前生效版本。
    ///
    /// # 参数
    /// * `order` - 调用方已用 `require_object` / `command_access.current` 证明的来源采购单
    /// * `expected_lock_version` - 调用方持有的乐观锁版本
    ///
    /// # 返回
    /// 返回原采购单及其当前可变更基准版本。
    ///
    /// # 错误
    /// 版本冲突、单据未生效或基准版本缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 不得在本方法内按主键重读采购单；不可见对象必须在进入前得到统一 NotFound。
    pub async fn load_changeable_order(
        &self,
        order: PurchaseOrder,
        expected_lock_version: u64,
    ) -> Result<(PurchaseOrder, PurchaseOrderRevision)> {
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
    /// # 参数
    /// * `purchase_order_id` - 已证明可见的来源采购单主键
    ///
    /// # 返回
    /// 无进行中变更时返回 `Ok(())`。
    ///
    /// # 错误
    /// 仓储失败或已存在进行中变更时返回错误。
    ///
    /// # 关键业务约束
    /// 调用方必须先按来源采购单写动作证明可见性，避免越权请求读到进行中冲突。
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

    /// 读取变更主表，保持调用方指定的事务执行器与缺失错误。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回变更单实体。
    ///
    /// # 错误
    /// 不存在时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 本读取只为取得来源采购单主键；状态、版本校验必须在来源单范围证明之后。
    pub async fn load_change(&self, id: &str, executor: &mut dyn Executor) -> Result<PurchaseChangeOrder> {
        self.db
            .purchase_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".into()))
    }
}

/// 在来源采购单范围已证明后，校验草稿变更单的版本与可提交状态。
///
/// # 参数
/// * `change` - 已读取且来源采购单已证明可见的变更单
/// * `expected_lock_version` - 调用方持有的乐观锁版本
///
/// # 返回
/// 版本一致且仍为草稿时返回原变更单。
///
/// # 错误
/// 版本冲突或非草稿时返回冲突。
///
/// # 关键业务约束
/// 不得在范围证明之前调用；不可见来源单不得暴露“已提交”或版本冲突。
pub fn lock_draft_change(
    change: PurchaseChangeOrder,
    expected_lock_version: u64,
) -> Result<PurchaseChangeOrder> {
    change
        .ensure_expected_version(expected_lock_version)
        .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
    change
        .ensure_draft_for_submission()
        .map_err(|_| Error::ConflictError("变更单已提交，请勿重复提交".to_string()))?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::lock_draft_change;
    use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseChangeOrderData};
    use crate::Error;
    use erp_core::ids::{
        PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
    };

    /// 构造测试用草稿变更单。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可提交的草稿变更单。
    ///
    /// # 错误
    /// 构造失败时 panic。
    ///
    /// # 关键业务约束
    /// 仅用于证明范围证明之后的状态错误，不替代来源采购单授权。
    fn draft_change() -> PurchaseChangeOrder {
        PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "成本上涨".into(),
            },
            "user-1",
        )
        .expect("草稿必须可构造")
    }

    /// 草稿且版本一致时允许提交；版本冲突与非草稿不得先于授权暴露。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 本函数只覆盖授权之后的状态/版本错误映射。
    #[test]
    fn lock_draft_change_maps_version_and_status_after_authorization() {
        let change = draft_change();
        let version = change.base.version;
        let locked = lock_draft_change(change, version).expect("草稿必须可锁定");
        assert_eq!(locked.base.id, "pco-1");

        let mismatch = lock_draft_change(draft_change(), version.saturating_add(1)).unwrap_err();
        assert!(matches!(mismatch, Error::ConflictError(message) if message.contains("刷新后重试")));

        let mut submitted = draft_change();
        submitted
            .start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "user-1")
            .expect("必须能进入审批");
        let submitted_version = submitted.base.version;
        let already_submitted = lock_draft_change(submitted, submitted_version).unwrap_err();
        assert!(matches!(
            already_submitted,
            Error::ConflictError(message) if message.contains("请勿重复提交")
        ));
    }
}
