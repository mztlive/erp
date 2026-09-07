//! 验收根流程使用的跨域履约进度事实读取，贯穿调用方 Executor。
use super::acceptance_eligibility::{build_line_eligibilities, so_line_ids, EligibilityGroupSources};
use crate::{Error, Result};
use erp_core::ids::SalesOrderId;
use erp_fulfillment::entity::fulfillment::{AcceptanceProgress, FulfillmentFactType, ServiceFulfillment};
use erp_fulfillment::repository::FulfillmentExt;
use erp_sales::{entity::sales_order::BusinessType, repository::SalesOrderExt};
use mongodb::Database;

/// 验收过账/冲正后读取履约进度投影（§4.3.1：实物与服务「客户验收通过即履约完成」）。
///
/// 净验收（APPLY − REVERSE）、剩余可验收与进度派生全部由领域投影
/// `AcceptanceProgress` 执行（与验收工作台同一规则源）：全部明细验收通过 →
/// 已完成；部分通过 → 部分履约；否则 → 未开始。数量错误向上传递，不得静默
/// 降为零。卡券销售单进度由履约期限到期任务写入（§4.3.1），本函数不触碰。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `sales_order_id` - 销售单
///
/// # 返回
/// 返回可派生的进度及剩余可验收标记；None 表示不得刷新销售。
pub async fn load_customer_acceptance_progress(
    db: &Database,
    session: &mut dyn persistence_core::Executor,
    sales_order_id: &SalesOrderId,
) -> Result<Option<AcceptanceProgress>> {
    let order = db
        .sales_orders()
        .find_by_id(sales_order_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    if order.business_type != BusinessType::GoodsService {
        return Ok(None);
    }
    let revision_id = order
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::NotFound("销售单没有生效版本".to_string()))?;
    let revision = db
        .sales_order_revisions()
        .find_by_id(&revision_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("销售生效版本不存在".to_string()))?;
    let revision_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&revision.base.id.clone().into(), session)
        .await?;
    let revision_line_ids: Vec<erp_core::ids::SalesOrderRevisionLineId> = revision_lines
        .iter()
        .map(|line| line.base.id.clone().into())
        .collect();
    let goods_service_lines = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&revision_line_ids, session)
        .await?;
    let deliveries = db
        .fulfillment()
        .list_acceptance_eligible_deliveries(sales_order_id, session)
        .await?;
    let delivery_ids: Vec<erp_core::ids::DeliveryId> = deliveries
        .iter()
        .map(|delivery| delivery.base.id.clone().into())
        .collect();
    let delivery_lines = db
        .fulfillment()
        .delivery_lines_by_delivery_ids(&delivery_ids, session)
        .await?;
    let sales_order_line_ids = so_line_ids(&revision_lines);
    let electronic = db
        .fulfillment()
        .list_confirmed_electronic_deliveries(&sales_order_line_ids, session)
        .await?;
    let service = db
        .fulfillment()
        .list_confirmed_service_fulfillments(&sales_order_line_ids, session)
        .await?
        .into_iter()
        .filter(ServiceFulfillment::is_acceptance_eligible)
        .collect::<Vec<_>>();
    let delivery_allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            FulfillmentFactType::Delivery,
            &delivery_lines
                .iter()
                .map(|line| line.base.id.clone())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let electronic_allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            FulfillmentFactType::ElectronicDelivery,
            &electronic
                .iter()
                .map(|record| record.base.id.clone())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let service_allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            FulfillmentFactType::ServiceFulfillment,
            &service
                .iter()
                .map(|record| record.base.id.clone())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let lines = build_line_eligibilities(&EligibilityGroupSources {
        revision_lines: &revision_lines,
        goods_service_lines: &goods_service_lines,
        deliveries: &deliveries,
        delivery_lines: &delivery_lines,
        electronic: &electronic,
        service: &service,
        delivery_allocations: &delivery_allocations,
        electronic_allocations: &electronic_allocations,
        service_allocations: &service_allocations,
    })?;
    let Some(progress) = AcceptanceProgress::derive(&lines) else {
        return Ok(None);
    };
    Ok(Some(progress))
}
