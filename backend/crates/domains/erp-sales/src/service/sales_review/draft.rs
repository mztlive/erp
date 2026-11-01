//! 销售变更草稿读取与同一执行器内的原单编辑。

use erp_core::ids::{
    SalesChangeOrderId, SalesOrderRevisionLineId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId,
};
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::SalesReviewService;
use crate::dto::sales_review::{SalesChangeDraftView, SaveSalesChangeDraftRequest};
use crate::entity::sales_order::{
    SalesOrderLine, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData,
};
use crate::entity::sales_review::{SalesChangeOrder, SalesChangeTerms};
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
        self.draft_view(&change, &copy, &lines, executor).await
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
        if let Some(terms) = request.terms {
            terms.apply(&mut copy, actor_id)?;
        }
        let stable = self.new_change_lines(&change, &old_lines, &request.lines, executor).await?;
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
            service.create_working_copy(&stable, &copy, &lines, executor).await?;
        } else {
            service.persist_saved_working_copy(&stable, old_lines, &lines, &mut copy, executor).await?;
        }
        self.db.sales_change_orders().update(&mut change, executor).await?;
        self.draft_view(&change, &copy, &lines, executor).await
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

impl SalesReviewService {
    /// 从已验证事实生成草稿视图，并避开历史已占用的行号。
    async fn draft_view(
        &self,
        change: &SalesChangeOrder,
        copy: &SalesOrderWorkingCopy,
        lines: &[SalesOrderWorkingCopyLine],
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeDraftView> {
        let stable =
            self.db.sales_order_lines().list_lines_by_order(&change.sales_order_id, executor).await?;
        let next_line_no = stable
            .iter()
            .map(|line| line.line_no)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("销售明细行号已达上限".into()))?;
        let base = self
            .db
            .sales_order_revisions()
            .find_by_id(change.base_revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单基准版本不存在".into()))?;
        Ok(SalesChangeDraftView {
            base_gross_amount: base.gross_amount,
            base_lines: self.base_lines(change, executor).await?,
            version: change.base.version,
            working_copy_version: copy.base.version,
            content_hash: copy.content_hash.clone(),
            reason: change.reason.clone(),
            terms: SalesChangeTerms::from_copy(copy),
            business_remark: copy.business_remark.clone(),
            next_line_no,
            lines: lines.iter().map(SalesOrderWorkingCopyLine::draft_data).collect::<Result<_>>()?,
        })
    }

    /// 读取基准明细，草稿保存不得覆盖界面的变更前依据。
    async fn base_lines(
        &self,
        change: &SalesChangeOrder,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderWorkingCopyLineData>> {
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(&change.base_revision_id, executor)
            .await?;
        let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
        let goods = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&ids, executor)
            .await?;
        lines
            .iter()
            .map(|line| {
                let detail = goods
                    .iter()
                    .find(|goods| goods.revision_line_id.as_ref() == line.base.id)
                    .ok_or_else(|| Error::NotFound("基准商品明细不存在".into()))?;
                Ok(line.to_goods_working_copy_data(detail)?)
            })
            .collect()
    }

    /// 为本次新增商品分配稳定行；命名空间及唯一索引禁止借用其他订单身份。
    async fn new_change_lines(
        &self,
        change: &SalesChangeOrder,
        old: &[SalesOrderWorkingCopyLine],
        target: &[SalesOrderWorkingCopyLineData],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderLine>> {
        let stable =
            self.db.sales_order_lines().list_lines_by_order(&change.sales_order_id, executor).await?;
        change.new_lines(old, target, &stable)
    }
}
