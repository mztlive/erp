//! 销售生命周期中的询价数量占用；每个步骤只使用调用方事务执行器。
use erp_core::common::time::Instant;
use erp_core::ids::CustomerAccountId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_order::inquiry_source::InquirySalesLineFact;
use crate::entity::sales_order::{SalesOrderGoodsServiceLineRevision, SalesOrderRevisionLine};
use crate::repository::sales_order::inquiry_source::inquiry_sources;
use crate::service::sales_inquiry::SalesInquiryService;
use crate::{Error, Result};

/// 按完整草稿或提交重算原来源数量；后台刷新及价格编辑不产生重复占用。
/// # 参数
/// `order_id`、`customer` 为当前销售身份，`lines` 为即将保存的完整行，`executor` 为原事务。
/// # 返回
/// 无来源订单保持原行为；变更的来源数量与销售写入原子提交。
/// # 错误
/// 来源损坏、客户/SKU/地址/交期变化或增加超过已接受范围时拒绝。
pub async fn reconcile(
    db: &Database,
    order_id: &str,
    customer: &CustomerAccountId,
    lines: &[InquirySalesLineFact],
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(mut source) = inquiry_sources(db).find_by_id(order_id, executor).await? else {
        return Ok(());
    };
    let updated = source.reconciled(customer, lines)?;
    if source.consumed || source.released {
        return Err(Error::BusinessLogicError("该销售询价来源已消耗或释放，不能直接编辑".into()));
    }
    if source.lines == updated {
        return Ok(());
    }
    let mut inquiry = SalesInquiryService::new(db.clone()).require(&source.inquiry_id, executor).await?;
    source.lines = updated;
    let selections = source.selections();
    source.released = selections.is_empty();
    inquiry.reconcile_quantities(order_id, selections, Instant::now())?;
    SalesInquiryService::new(db.clone()).save(&mut inquiry, executor).await?;
    inquiry_sources(db).update(&mut source, executor).await?;
    Ok(())
}

/// 草稿作废仅释放未消耗占用，不以撤回或驳回状态释放。
/// # 参数
/// `order_id` 为作废销售，`executor` 为原事务。
/// # 返回
/// 无来源或已释放时幂等成功。
/// # 错误
/// 正式消耗不可凭草稿状态释放；仓储失败时整单回滚。
pub async fn release(db: &Database, order_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let Some(mut source) = inquiry_sources(db).find_by_id(order_id, executor).await? else {
        return Ok(());
    };
    if source.released {
        return Ok(());
    }
    let mut inquiry = SalesInquiryService::new(db.clone()).require(&source.inquiry_id, executor).await?;
    inquiry.release_quantities(order_id)?;
    source.released = true;
    SalesInquiryService::new(db.clone()).save(&mut inquiry, executor).await?;
    inquiry_sources(db).update(&mut source, executor).await?;
    Ok(())
}

/// 销售首次生效将原占用转为消耗，不重复扣减且不受询价后续关闭影响。
/// # 参数
/// `order_id` 为生效销售，`executor` 为审批持有的原事务。
/// # 返回
/// 无来源或已消耗时幂等成功。
/// # 错误
/// 原占用不存在或已释放时拒绝，不能凭状态重建来源。
pub async fn consume(db: &Database, order_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let Some(mut source) = inquiry_sources(db).find_by_id(order_id, executor).await? else {
        return Ok(());
    };
    if source.consumed {
        return Ok(());
    }
    let mut inquiry = SalesInquiryService::new(db.clone()).require(&source.inquiry_id, executor).await?;
    inquiry.consume_quantities(order_id)?;
    source.consumed = true;
    SalesInquiryService::new(db.clone()).save(&mut inquiry, executor).await?;
    inquiry_sources(db).update(&mut source, executor).await?;
    Ok(())
}

/// 正式改单验证需求来源；减量不恢复已消耗余额，保留原快照和下游处理链。
/// # 参数
/// `customer`、`lines`、`goods` 为将生效的新正式版本，`executor` 为变更事务。
/// # 返回
/// 无来源保持原行为；来源商品仅允许原身份、原地址及已消耗量内变更。
/// # 错误
/// 来源不完整、正式增量、换客户/SKU/地址/交期时拒绝，须另行完成询价。
pub async fn ensure_formal_change(
    db: &Database,
    order_id: &str,
    customer: &CustomerAccountId,
    lines: &[SalesOrderRevisionLine],
    goods: &[SalesOrderGoodsServiceLineRevision],
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(source) = inquiry_sources(db).find_by_id(order_id, executor).await? else {
        return Ok(());
    };
    source.ensure_formal_change(customer, &revision_facts(lines, goods)?)
}

fn revision_facts(
    lines: &[SalesOrderRevisionLine],
    goods: &[SalesOrderGoodsServiceLineRevision],
) -> Result<Vec<InquirySalesLineFact>> {
    goods
        .iter()
        .map(|goods| {
            let line = lines
                .iter()
                .find(|line| line.base.id == goods.revision_line_id.as_ref())
                .ok_or_else(|| Error::BusinessLogicError("销售版本商品缺少稳定需求行".into()))?;
            Ok(InquirySalesLineFact {
                sales_order_line_id: line.sales_order_line_id.clone(),
                sku_id: Some(goods.sku_id.clone()),
                sku_revision_id: Some(goods.sku_revision_id.clone()),
                fulfillment_due_at: Some(goods.fulfillment_due_at),
                quantity: Some(goods.quantity),
                delivery_allocations: goods.delivery_allocations.clone(),
            })
        })
        .collect()
}
