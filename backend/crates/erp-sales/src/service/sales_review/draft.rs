//! 销售变更草稿读取与同一执行器内的原单编辑。

use erp_core::ids::{SalesChangeOrderId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId};
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::SalesReviewService;
use crate::dto::sales_review::{SalesChangeDraftView, SaveSalesChangeDraftRequest};
use crate::entity::sales_order::{SalesOrderWorkingCopy, SalesOrderWorkingCopyLine};
use crate::entity::sales_review::SalesChangeOrder;
use crate::ports::sales_order::SellableSkuPort;
use crate::repository::prelude::*;
use crate::repository::{SalesOrderExt, SalesReviewExt};
use crate::service::sales_order::SalesOrderService;
use crate::{Error, Result};

impl SalesReviewService {
    /// 在调用方执行器内读取可编辑的原变更内容。
    ///
    /// # 参数
    /// * `id` - 原销售变更单身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回原单与工作副本版本及全部目标明细。
    ///
    /// # 错误
    /// 缺单、工作副本错配或不是草稿时拒绝。
    pub async fn draft(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesChangeDraftView> {
        let (change, copy, lines) = self.load_draft(id, executor).await?;
        draft_view(&change, &copy, &lines)
    }

    /// 在调用方事务内保存原变更内容，已提交副本保持不可变。
    ///
    /// # 参数
    /// * `id` - 原销售变更单身份
    /// * `request` - 两项期望版本、变更原因与完整目标明细
    /// * `actor_id` - 当前编辑人
    /// * `catalog` - 精确商品修订资格接口
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 返回保存后的完整目标草稿。
    ///
    /// # 错误
    /// 状态、版本、完整明细或商品资格不成立，及仓储失败时拒绝。
    pub async fn save_draft(
        &self,
        id: &str,
        request: SaveSalesChangeDraftRequest,
        actor_id: &str,
        catalog: &dyn SellableSkuPort,
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeDraftView> {
        request.validate()?;
        let (mut change, old_copy, old_lines) = self.load_draft(id, executor).await?;
        change.edit_draft(request.expected_version, request.reason, actor_id)?;
        let mut copy = old_copy.editable_change_copy(
            &change,
            request.expected_working_copy_version,
            SalesOrderWorkingCopyId::new(next_id()),
            actor_id,
        )?;
        let target = request
            .lines
            .into_iter()
            .map(|line| (SalesOrderWorkingCopyLineId::new(next_id()), line))
            .collect();
        let lines = copy.edit_change_lines(id, &old_lines, target, request.business_remark, actor_id)?;
        let service = SalesOrderService::new(self.db.clone());
        service
            .ensure_sellable_refs(&SalesOrderService::sellable_working_copy_refs(&lines)?, catalog, executor)
            .await?;
        if old_copy.is_submitted() {
            service.create_working_copy(&[], &copy, &lines, executor).await?;
        } else {
            service.persist_saved_working_copy(&[], old_lines, &lines, &mut copy, executor).await?;
        }
        self.db.sales_change_orders().update(&mut change, executor).await?;
        draft_view(&change, &copy, &lines)
    }

    /// 加载原单与对应的完整工作副本，状态由拥有领域校验。
    async fn load_draft(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SalesChangeOrder, SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>)> {
        let change = self
            .db
            .sales_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".into()))?;
        if !change.is_draft() {
            return Err(Error::ConflictError("请先撤回销售变更审批，再修改原单".into()));
        }
        let copy = self
            .db
            .sales_order_working_copies()
            .find_resubmittable_sales_change_copy(
                &change.sales_order_id,
                &SalesChangeOrderId::new(id),
                executor,
            )
            .await?
            .ok_or_else(|| Error::NotFound("销售变更工作副本不存在".into()))?;
        copy.ensure_change_editable(&change)?;
        let lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&SalesOrderWorkingCopyId::new(&copy.base.id), executor)
            .await?;
        Ok((change, copy, lines))
    }
}

/// 从已验证的领域事实生成草稿视图。
fn draft_view(
    change: &SalesChangeOrder,
    copy: &SalesOrderWorkingCopy,
    lines: &[SalesOrderWorkingCopyLine],
) -> Result<SalesChangeDraftView> {
    Ok(SalesChangeDraftView {
        version: change.base.version,
        working_copy_version: copy.base.version,
        content_hash: copy.content_hash.clone(),
        reason: change.reason.clone(),
        business_remark: copy.business_remark.clone(),
        lines: lines.iter().map(SalesOrderWorkingCopyLine::draft_data).collect::<Result<_>>()?,
    })
}
