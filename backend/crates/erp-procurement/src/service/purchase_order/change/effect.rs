//! 采购变更生效的本域准备与事务内步骤；外域差额不进入本计划。
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::dto::purchase_order::PurchaseChangeEffectResult;
use crate::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseOrder, PurchaseOrderRevision,
    PurchaseOrderRevisionLine,
};
use crate::repository::PurchaseOrderExt;
use crate::service::purchase_order::PurchaseOrderService;
use crate::{Error, Result};
impl PurchaseOrderService {
    /// 准备采购变更生效事务所需的修订、差额和响应引用。
    ///
    /// # 参数
    /// * `change` - 已通过最终审批动作校验的采购变更单
    /// * `submission_id` - 当前冻结且待生效的变更提交主键
    ///
    /// # 返回
    /// 返回可直接进入事务的完整写聚合及响应所需稳定引用。
    ///
    /// # 错误
    /// 原采购单、基准版本或提交缺失，基准版本漂移，或修订与差额构建
    /// 失败时返回错误。
    ///
    /// # 关键业务约束
    /// 这里只准备不可变写内容；采购、销售 guard、allocation 和任务的可见性
    /// 由后续事务保证。
    pub async fn prepare_purchase_effective_change(
        &self,
        change: &PurchaseChangeOrder,
        submission_id: &str,
    ) -> Result<(EffectiveChangeWrite, PurchaseOrderRevision)> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        change
            .ensure_base_revision_current(order.stable.current_revision_id.as_deref())
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let (submission, lines) = self.load_pending_change_submission(submission_id).await?;
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) =
            self.build_change_revision(&order, &submission, &lines, revision_no).await?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        Ok((
            EffectiveChangeWrite { order, change: change.clone(), submission, revision, revision_lines },
            base_revision,
        ))
    }
    /// 加载待生效的变更提交及其明细。
    ///
    /// # 错误
    /// 提交不存在或已处理时返回错误。
    async fn load_pending_change_submission(
        &self,
        submission_id: &str,
    ) -> Result<(PurchaseChangeSubmission, Vec<crate::entity::purchase_order::PurchaseChangeSubmissionLine>)>
    {
        let submission = self
            .db
            .purchase_change_submissions()
            .find_by_id(submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
        submission
            .ensure_pending()
            .map_err(|_| Error::ConflictError("变更提交已处理，请勿重复生效".to_string()))?;
        let lines = self
            .db
            .purchase_order()
            .list_change_submission_lines(&submission.base.id.clone().into(), &mut NoTransaction)
            .await?;
        Ok((submission, lines))
    }
    /// 客户端直接生效失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无成功返回值。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_effect() -> Result<PurchaseChangeEffectResult> {
        Err(Error::ConflictError("采购变更生效只能由审批最终通过动作执行，客户端不得直接生效".to_string()))
    }
}

/// 全部字段为采购拥有的单据、冻结提交和新版本；流程不得重新构造计划身份。
pub struct EffectiveChangeWrite {
    /// 原采购单，当前版本切换发生在 allocations 持久化之后。
    pub order: PurchaseOrder,
    /// 本次变更单，状态先在原审计构造后迁移。
    pub change: PurchaseChangeOrder,
    /// 待通过的冻结提交。
    pub submission: PurchaseChangeSubmission,
    /// 本次生效版本。
    pub revision: PurchaseOrderRevision,
    /// 待绑定当前销售行的采购版本行。
    pub revision_lines: Vec<PurchaseOrderRevisionLine>,
}
impl EffectiveChangeWrite {
    /// 标记变更已生效；调用方保持原审计构造之后、来源销售 guard 之前的时点。
    ///
    /// # 参数
    /// * `actor_id` - 最终通过执行人
    ///
    /// # 返回
    /// 变更单进入生效并记下新版本后无返回值。
    ///
    /// # 错误
    /// 变更单不是审批中时，实体错误经 `?` 变为 `Logic`。
    pub fn mark_effective(&mut self, actor_id: &str) -> Result<()> {
        Ok(self.change.apply_effective(self.revision.base.id.clone().into(), actor_id)?)
    }
    /// 写本次正式版本和版本行，不改变其他域或开启事务。
    ///
    /// # 参数
    /// * `db` - 采购数据库
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 版本和版本行写入成功时无返回值。
    ///
    /// # 错误
    /// 仓储插入失败时返回对应错误。
    pub async fn persist_revision(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        Ok(db
            .purchase_order()
            .create_effective_revision(&self.revision, &self.revision_lines, executor)
            .await?)
    }
    /// 原分配持久化成功后切当前版本并 CAS 更新采购单。
    ///
    /// # 参数
    /// * `db` - 采购数据库
    /// * `actor_id` - 最终通过执行人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 当前版本指针更新并 CAS 成功时无返回值。
    ///
    /// # 错误
    /// 采购单状态不允许应用变更时返回 `Logic`；版本冲突或其他仓储失败时返回对应错误。
    pub async fn persist_current_order(
        &mut self,
        db: &Database,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.order.apply_change_revision(self.revision.base.id.clone().into(), actor_id)?;
        db.purchase_orders().update(&mut self.order, executor).await?;
        Ok(())
    }
    /// 原财务差额写入成功后通过冻结提交并 CAS 更新。
    ///
    /// # 参数
    /// * `db` - 采购数据库
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 提交改为已通过并 CAS 成功时无返回值。
    ///
    /// # 错误
    /// 提交不是待审核时返回 `Logic`；仓储更新失败时返回对应错误。
    pub async fn persist_approved_submission(
        &mut self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.submission.approve()?;
        db.purchase_change_submissions().update(&mut self.submission, executor).await?;
        Ok(())
    }
    /// 写已生效变更状态，不生成审计或产生财务副作用。
    ///
    /// # 参数
    /// * `db` - 采购数据库
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 变更单 CAS 成功时无返回值。
    ///
    /// # 错误
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_change(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        Ok(db.purchase_change_orders().update(&mut self.change, executor).await?)
    }
}
