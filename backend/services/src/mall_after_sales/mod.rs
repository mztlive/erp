//! 域 D30 `mall_after_sales` 服务编排（W25 售后结果：退款与余额恢复）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 商城退款（§8.4 第 3 条：`REFUND_SUCCEEDED` 事实 + 退款头 + 商品退款行 +
//!   沿原支付来源的 `APPLY` 分配 + 消费反向事实原子写入）→
//!   `database::Transactional::with_transaction`；
//! - 卡券余额恢复（§8.4 第 4 条：恢复事实 + 指向原 CARD 退款分配的恢复分配）→
//!   同一事务；
//! - 列表/详情只读查询 → `&mut NoTransaction`。
//!
//! 幂等（§6.17/§6.18）：`business_fact_key`/`inbox_message_id` 唯一，重复接收
//! 只返回既有事实；退款/恢复身份（商城 + 单号 + 版本）唯一索引兜底。
//!
//! 跨域协作只调对方 Repository（P3-service-api.md §2）：
//! - D29 `MallOrderExt`：关键事实、订单、支付来源与消费事实；
//! - D28 `CardInstanceExt`：卡实例归属校验（余额恢复只回到原支付卡实例）。
//!
//! 已知冻结实体缺陷（P1）：`MallAfterSalesRequest` 的 `created_at` 与
//! `BaseModel.created_at` 重名，头表暂不可持久化；售后请求只走投影查询，
//! 状态推进待地基修订后补齐（见 PR「需要协调人处理的事项」）。

use database::{
    AccessControlExt, CardInstanceExt, MallAfterSalesExt, MallOrderExt, NoTransaction, Transactional,
};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, MallAfterSalesRequestId, MallBalanceRestorationAllocationId, MallBalanceRestorationId,
    MallConsumptionEntryId, MallOrderFactId, MallRefundAllocationId, MallRefundId, MallRefundLineId,
};
use entities::mall_after_sales::{
    AllocationAction, MallBalanceRestoration, MallBalanceRestorationAllocation,
    MallBalanceRestorationAllocationData, MallBalanceRestorationData, MallRefund, MallRefundAllocation,
    MallRefundAllocationData, MallRefundData, MallRefundLine, MallRefundLineData,
};
use entities::mall_order::{
    AttributionStatus, ConsumptionDirection, FactType, MallConsumptionEntry, MallConsumptionEntryData,
    MallOrderFact, MallOrderFactData, MallPaymentSource, ProcessingStatus,
};
use entities::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    AfterSalesRequestListParams, AfterSalesRequestView, MallBalanceRestorationListParams,
    MallBalanceRestorationView, MallRefundListParams, MallRefundView, PageView,
    ReceiveBalanceRestorationRequest, ReceiveRefundFactRequest, ReceivedFactView,
};

/// 售后请求列表筛选条件类型（经 `MallAfterSalesExt` 关联类型跨 crate 可达）。
type AfterSalesRequestFilter = <mongodb::Database as MallAfterSalesExt>::MallAfterSalesRequestFilter;

/// 商城售后域服务：退款与余额恢复事实接收、售后查询。
pub struct MallAfterSalesService {
    db: Database,
}

impl MallAfterSalesService {
    /// 创建服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 接收商城退款成功事实（§8.4 第 3 条，幂等）。
    ///
    /// 一个事务内写入：`REFUND_SUCCEEDED` 事实 + 退款头 + 商品退款行 + 初始
    /// `APPLY` 分配 + 逐分配消费反向事实 + 审计；累计数量/金额上限按原消费
    /// 净额校验（§6.18）。
    ///
    /// # 参数
    /// * `req` - 退款事实接收请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    ///
    /// # 错误
    /// * `BusinessLogicError` - 原支付缺失/未归集、行金额守恒不成立、
    ///   累计退款超限、分配与行/消费不匹配
    /// * `ConflictError` - 唯一键冲突
    /// * `ValidationError` - 请求体校验失败
    pub async fn receive_refund(
        &self,
        req: ReceiveRefundFactRequest,
        actor: &AuditActor,
    ) -> Result<ReceivedFactView> {
        req.validate()?;
        if let Some(fact) = self
            .db
            .mall_order_facts()
            .find_by_business_fact_key(&req.business_fact_key, &mut NoTransaction)
            .await?
        {
            return Ok(hit_view(&fact));
        }
        if let Some(fact) = self
            .db
            .mall_order_facts()
            .find_by_inbox_message(
                &InboxMessageId::new(req.inbox_message_id.clone()),
                &mut NoTransaction,
            )
            .await?
        {
            return Ok(hit_view(&fact));
        }
        if let Some(existing) = self
            .db
            .mall_refunds()
            .find_by_identity(
                &req.mall_id,
                &req.external_refund_no,
                &req.external_refund_version,
                &mut NoTransaction,
            )
            .await?
        {
            if let Some(fact) = self
                .db
                .mall_order_facts()
                .find_by_id(&existing.mall_order_fact_id, &mut NoTransaction)
                .await?
            {
                return Ok(hit_view(&fact));
            }
            return Ok(ReceivedFactView {
                fact_id: existing.mall_order_fact_id.to_string(),
                fact_type: FactType::RefundSucceeded,
                processing_status: ProcessingStatus::Attributed,
                idempotent_hit: true,
            });
        }

        let original = self
            .load_original_payment(
                &req.mall_id,
                &req.external_order_no,
                &req.original_payment_fact_id,
            )
            .await?;
        let order = self
            .db
            .mall_orders()
            .find_by_payment_fact(
                &MallOrderFactId::new(original.base.id.clone()),
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::BusinessLogicError("原支付未形成商城订单".to_string()))?;
        let order_id: entities::ids::MallOrderId = order.base.id.clone().into();
        let order_items = self
            .db
            .mall_order_items()
            .list_items_by_order(&order_id, &mut NoTransaction)
            .await?;

        let fact_id = MallOrderFactId::new(next_id());
        let refund_id = MallRefundId::new(next_id());
        let mut fact = MallOrderFact::new(
            fact_id.clone(),
            MallOrderFactData {
                mall_id: req.mall_id.clone(),
                source_event_id: req.source_event_id.clone(),
                inbox_message_id: InboxMessageId::new(req.inbox_message_id.clone()),
                fact_type: FactType::RefundSucceeded,
                business_fact_key: req.business_fact_key.clone(),
                external_order_no: req.external_order_no.clone(),
                external_order_version: req.external_order_version.clone(),
                after_sales_request_id: Some(req.after_sales_request_id.clone()),
                original_payment_fact_id: Some(req.original_payment_fact_id.clone()),
                occurred_at: Instant::from_unix_secs(req.occurred_at as i64),
                received_at: Instant::from_unix_secs(req.received_at as i64),
                data_source: req.data_source,
                raw_payload_reference: req.raw_payload_reference.clone(),
            },
        )?;
        fact.update_processing_status(ProcessingStatus::Attributed)?;
        let refund = MallRefund::new(
            refund_id.clone(),
            MallRefundData {
                mall_order_fact_id: fact_id.clone(),
                after_sales_request_id: req.after_sales_request_id.clone(),
                mall_id: req.mall_id.clone(),
                external_refund_no: req.external_refund_no.clone(),
                external_refund_version: req.external_refund_version.clone(),
                mall_order_id: order.base.id.clone().into(),
                refund_amount: Amount::from_str(&req.refund_amount)?,
                refunded_at: Instant::from_unix_secs(req.refunded_at as i64),
            },
        )?;

        let (lines, allocations, reversal_entries) = self
            .build_refund_plan(&req, &order_items, &refund, &fact_id)
            .await?;
        let audit =
            actor
                .clone()
                .resource_log("mall_refund.received", "mall_refund", refund.base.id.clone())?;
        let fact_for_tx = fact.clone();
        let refund_for_tx = refund.clone();
        let lines_for_tx = lines.clone();
        let allocations_for_tx = allocations.clone();
        let entries_for_tx = reversal_entries.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_order_facts().create(&fact_for_tx, session).await?;
                    db.mall_after_sales()
                        .create_refund_with_lines_and_allocations(
                            &refund_for_tx,
                            &lines_for_tx,
                            &allocations_for_tx,
                            session,
                        )
                        .await?;
                    for entry in &entries_for_tx {
                        db.mall_consumption_entries().create(entry, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReceivedFactView {
            fact_id: fact_id.to_string(),
            fact_type: FactType::RefundSucceeded,
            processing_status: ProcessingStatus::Attributed,
            idempotent_hit: false,
        })
    }

    /// 分页查询退款列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_order_id`/`after_sales_request_id` 筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_refund_list(&self, params: &MallRefundListParams) -> Result<PageView<MallRefundView>> {
        params.validate()?;
        let query = params.normalized()?;
        let mut refunds = if let Some(order_id) = &query.mall_order_id {
            self.db
                .mall_refunds()
                .list_by_order(order_id, &mut NoTransaction)
                .await?
        } else if let Some(request_id) = &query.after_sales_request_id {
            self.db
                .mall_refunds()
                .list_by_after_sales_request(request_id, &mut NoTransaction)
                .await?
        } else {
            Vec::new()
        };
        sort_refunds(&mut refunds, query.paging.sort_by, query.paging.sort_dir);
        let (items, total) = slice_page(refunds, query.paging, |refund| MallRefundView {
            id: refund.base.id.clone(),
            mall_order_fact_id: refund.mall_order_fact_id.to_string(),
            after_sales_request_id: refund.after_sales_request_id.to_string(),
            mall_id: refund.mall_id.clone(),
            external_refund_no: refund.external_refund_no.clone(),
            external_refund_version: refund.external_refund_version.clone(),
            mall_order_id: refund.mall_order_id.to_string(),
            refund_amount: refund.refund_amount.to_string(),
            refunded_at: refund.refunded_at.unix_secs() as u64,
            created_at: refund.base.created_at,
        });
        Ok(PageView {
            items,
            total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 接收卡券余额恢复事实（§8.4 第 4 条，幂等）。
    ///
    /// 一个事务内写入：`CARD_BALANCE_RESTORED` 事实 + 恢复头 + 指向原 CARD
    /// 退款分配的恢复分配 + 审计；每卡累计恢复不超过对应退款净额（§6.18）。
    ///
    /// # 参数
    /// * `req` - 余额恢复事实接收请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    ///
    /// # 错误
    /// * `BusinessLogicError` - 原支付缺失/未归集、分配与退款/卡实例不匹配、
    ///   累计恢复超限、分配合计与恢复金额不一致
    /// * `ConflictError` - 唯一键冲突
    /// * `ValidationError` - 请求体校验失败
    pub async fn receive_balance_restoration(
        &self,
        req: ReceiveBalanceRestorationRequest,
        actor: &AuditActor,
    ) -> Result<ReceivedFactView> {
        req.validate()?;
        if let Some(fact) = self
            .db
            .mall_order_facts()
            .find_by_business_fact_key(&req.business_fact_key, &mut NoTransaction)
            .await?
        {
            return Ok(hit_view(&fact));
        }
        if let Some(fact) = self
            .db
            .mall_order_facts()
            .find_by_inbox_message(
                &InboxMessageId::new(req.inbox_message_id.clone()),
                &mut NoTransaction,
            )
            .await?
        {
            return Ok(hit_view(&fact));
        }
        if let Some(existing) = self
            .db
            .mall_balance_restorations()
            .find_by_identity(
                &req.mall_id,
                &req.external_restoration_no,
                &req.version,
                &mut NoTransaction,
            )
            .await?
        {
            if let Some(fact) = self
                .db
                .mall_order_facts()
                .find_by_id(&existing.mall_order_fact_id, &mut NoTransaction)
                .await?
            {
                return Ok(hit_view(&fact));
            }
            return Ok(ReceivedFactView {
                fact_id: existing.mall_order_fact_id.to_string(),
                fact_type: FactType::CardBalanceRestored,
                processing_status: ProcessingStatus::Attributed,
                idempotent_hit: true,
            });
        }

        let _original = self
            .load_original_payment(
                &req.mall_id,
                &req.external_order_no,
                &req.original_payment_fact_id,
            )
            .await?;
        let fact_id = MallOrderFactId::new(next_id());
        let restoration_id = MallBalanceRestorationId::new(next_id());
        let mut fact = MallOrderFact::new(
            fact_id.clone(),
            MallOrderFactData {
                mall_id: req.mall_id.clone(),
                source_event_id: req.source_event_id.clone(),
                inbox_message_id: InboxMessageId::new(req.inbox_message_id.clone()),
                fact_type: FactType::CardBalanceRestored,
                business_fact_key: req.business_fact_key.clone(),
                external_order_no: req.external_order_no.clone(),
                external_order_version: req.external_order_version.clone(),
                after_sales_request_id: Some(req.after_sales_request_id.clone()),
                original_payment_fact_id: Some(req.original_payment_fact_id.clone()),
                occurred_at: Instant::from_unix_secs(req.occurred_at as i64),
                received_at: Instant::from_unix_secs(req.received_at as i64),
                data_source: req.data_source,
                raw_payload_reference: req.raw_payload_reference.clone(),
            },
        )?;
        fact.update_processing_status(ProcessingStatus::Attributed)?;
        let (allocations, mall_refund_id) = self
            .build_restoration_allocations(&req, &restoration_id, &req.after_sales_request_id)
            .await?;
        let restoration = MallBalanceRestoration::new(
            restoration_id.clone(),
            MallBalanceRestorationData {
                mall_order_fact_id: fact_id.clone(),
                after_sales_request_id: req.after_sales_request_id.clone(),
                mall_refund_id,
                mall_id: req.mall_id.clone(),
                external_restoration_no: req.external_restoration_no.clone(),
                version: req.version.clone(),
                restored_amount: Amount::from_str(&req.restored_amount)?,
                restored_at: Instant::from_unix_secs(req.restored_at as i64),
            },
        )?;
        restoration.ensure_allocation_total(&allocations)?;
        let audit = actor.clone().resource_log(
            "mall_balance_restoration.received",
            "mall_balance_restoration",
            restoration.base.id.clone(),
        )?;
        let fact_for_tx = fact.clone();
        let restoration_for_tx = restoration.clone();
        let allocations_for_tx = allocations.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_order_facts().create(&fact_for_tx, session).await?;
                    db.mall_after_sales()
                        .create_balance_restoration_with_allocations(
                            &restoration_for_tx,
                            &allocations_for_tx,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReceivedFactView {
            fact_id: fact_id.to_string(),
            fact_type: FactType::CardBalanceRestored,
            processing_status: ProcessingStatus::Attributed,
            idempotent_hit: false,
        })
    }

    /// 分页查询余额恢复列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`after_sales_request_id` 筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_balance_restoration_list(
        &self,
        params: &MallBalanceRestorationListParams,
    ) -> Result<PageView<MallBalanceRestorationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let mut restorations = if let Some(request_id) = &query.after_sales_request_id {
            self.db
                .mall_balance_restorations()
                .list_by_after_sales_request(request_id, &mut NoTransaction)
                .await?
        } else {
            Vec::new()
        };
        sort_restorations(&mut restorations, query.paging.sort_by, query.paging.sort_dir);
        let (items, total) = slice_page(restorations, query.paging, |restoration| {
            MallBalanceRestorationView {
                id: restoration.base.id.clone(),
                mall_order_fact_id: restoration.mall_order_fact_id.to_string(),
                after_sales_request_id: restoration.after_sales_request_id.to_string(),
                mall_refund_id: restoration.mall_refund_id.to_string(),
                mall_id: restoration.mall_id.clone(),
                external_restoration_no: restoration.external_restoration_no.clone(),
                version: restoration.version.clone(),
                restored_amount: restoration.restored_amount.to_string(),
                restored_at: restoration.restored_at.unix_secs() as u64,
                created_at: restoration.base.created_at,
            }
        });
        Ok(PageView {
            items,
            total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 分页查询售后请求列表（投影查询）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_id`/`mall_order_id`/`request_type`/`status` 筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn after_sales_request_list(
        &self,
        params: &AfterSalesRequestListParams,
    ) -> Result<PageView<AfterSalesRequestView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = AfterSalesRequestFilter {
            mall_id: query.mall_id,
            external_request_id: None,
            mall_order_id: query.mall_order_id,
            request_type: query.request_type,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_after_sales_requests()
            .search_after_sales_requests(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| AfterSalesRequestView {
                id: row.id,
                mall_id: row.mall_id,
                external_request_id: row.external_request_id,
                mall_order_id: row.mall_order_id.to_string(),
                request_type: row.request_type,
                status: row.status,
                reason: row.reason,
                created_at: row.created_at.unix_secs() as u64,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}

impl MallAfterSalesService {
    /// 加载并校验原支付事实（存在、`PAYMENT_SUCCEEDED`、同商城同订单、已归集）。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `external_order_no` - 商城订单号
    /// * `original_payment_fact_id` - 原支付事实 ID
    ///
    /// # 返回
    /// 返回原支付事实实体。
    ///
    /// # 错误
    /// 任一校验不通过返回 `BusinessLogicError`。
    async fn load_original_payment(
        &self,
        mall_id: &str,
        external_order_no: &str,
        original_payment_fact_id: &MallOrderFactId,
    ) -> Result<MallOrderFact> {
        let original = self
            .db
            .mall_order_facts()
            .find_by_id(original_payment_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("原支付事实不存在".to_string()))?;
        original.ensure_attributed_payment_for(mall_id, external_order_no)?;
        Ok(original)
    }

    /// 构建退款写入计划（行/分配/消费反向事实），并做守恒与累计上限校验。
    ///
    /// # 参数
    /// * `req` - 退款事实接收请求
    /// * `order_items` - 原订单商品明细
    /// * `refund` - 已构造的退款头
    /// * `fact_id` - 事实 ID
    ///
    /// # 返回
    /// 返回 `(退款行, 退款分配, 消费反向事实)` 三元组。
    ///
    /// # 错误
    /// 行金额守恒、明细归属或累计退款上限不满足时返回 `BusinessLogicError`。
    async fn build_refund_plan(
        &self,
        req: &ReceiveRefundFactRequest,
        order_items: &[entities::mall_order::MallOrderItem],
        refund: &MallRefund,
        fact_id: &MallOrderFactId,
    ) -> Result<(
        Vec<MallRefundLine>,
        Vec<MallRefundAllocation>,
        Vec<MallConsumptionEntry>,
    )> {
        let occurred = Instant::from_unix_secs(req.occurred_at as i64);
        let refund_id = MallRefundId::new(refund.base.id.clone());
        let mut lines = Vec::with_capacity(req.lines.len());
        for line in &req.lines {
            let item_belongs_to_order = order_items.iter().any(|item| {
                item.base.id == line.mall_order_item_id.as_ref()
                    && item.belongs_to_order(&refund.mall_order_id)
            });
            if !item_belongs_to_order {
                return Err(Error::BusinessLogicError(format!(
                    "退款明细不属于原订单: {}",
                    line.mall_order_item_id
                )));
            }
            lines.push(MallRefundLine::new(
                MallRefundLineId::new(next_id()),
                MallRefundLineData {
                    mall_refund_id: refund_id.clone(),
                    line_no: line.line_no,
                    mall_order_item_id: line.mall_order_item_id.clone(),
                    refunded_quantity: entities::money::Quantity::from_str(&line.refunded_quantity)?,
                    line_refund_amount: Amount::from_str(&line.line_refund_amount)?,
                },
            )?);
        }
        refund.ensure_line_total(&lines)?;

        let previous_refunds = self
            .db
            .mall_refunds()
            .list_by_order(&refund.mall_order_id, &mut NoTransaction)
            .await?;
        let previous_ids: Vec<entities::ids::MallRefundId> = previous_refunds
            .iter()
            .map(|refund| refund.base.id.clone().into())
            .collect();
        let previous_lines = self
            .db
            .mall_refund_lines()
            .list_by_refunds(&previous_ids, &mut NoTransaction)
            .await?;
        for line in &lines {
            let item = order_items
                .iter()
                .find(|item| {
                    line.targets_item(&entities::ids::MallOrderItemId::new(item.base.id.clone()))
                        && item.belongs_to_order(&refund.mall_order_id)
                })
                .expect("退款行实体已确认明细属于原订单");
            let refunded_amount = previous_lines
                .iter()
                .filter(|previous| previous.targets_item(&line.mall_order_item_id))
                .fold(line.line_refund_amount, |acc, previous| {
                    acc.checked_add(previous.line_refund_amount)
                });
            let refunded_quantity = previous_lines
                .iter()
                .filter(|previous| previous.targets_item(&line.mall_order_item_id))
                .fold(line.refunded_quantity.to_decimal(), |acc, previous| {
                    acc + previous.refunded_quantity.to_decimal()
                });
            let refunded_quantity = entities::money::Quantity::try_from(refunded_quantity)?;
            if !item.allows_cumulative_refund(refunded_quantity, refunded_amount) {
                return Err(Error::BusinessLogicError(format!(
                    "商品 {} 累计退款超过原支付数量或金额",
                    line.mall_order_item_id
                )));
            }
        }

        let mut allocations = Vec::with_capacity(req.allocations.len());
        let mut reversal_entries = Vec::with_capacity(req.allocations.len());
        for allocation in &req.allocations {
            let line = lines
                .iter()
                .find(|line| line.line_no == allocation.line_no)
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!("分配引用的退款行不存在: {}", allocation.line_no))
                })?;
            let original_entry = self
                .db
                .mall_consumption_entries()
                .find_by_id(&allocation.original_consumption_entry_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("原消费事实不存在".to_string()))?;
            if !original_entry
                .matches_refund_source(&line.mall_order_item_id, &allocation.original_payment_source_id)
            {
                return Err(Error::BusinessLogicError(
                    "退款分配必须引用原商品与原支付来源的消费事实".to_string(),
                ));
            }
            let amount = Amount::from_str(&allocation.allocated_refund_amount)?;
            let accrued = self
                .refunded_net_for_entry(&allocation.original_consumption_entry_id)
                .await?
                .checked_add(amount);
            if !original_entry.allows_cumulative_refund(accrued) {
                return Err(Error::BusinessLogicError(format!(
                    "原消费累计退款不得超过原消费金额: {}",
                    original_entry.base.id
                )));
            }
            let reversal = MallConsumptionEntry::new(
                MallConsumptionEntryId::new(next_id()),
                MallConsumptionEntryData {
                    mall_order_fact_id: fact_id.clone(),
                    mall_order_item_id: original_entry.mall_order_item_id.clone(),
                    mall_payment_source_id: original_entry.mall_payment_source_id.clone(),
                    direction: ConsumptionDirection::ConsumptionReversal,
                    amount,
                    customer_id: original_entry.customer_id.clone(),
                    origin_sales_order_id: original_entry.origin_sales_order_id.clone(),
                    sales_order_line_id: original_entry.sales_order_line_id.clone(),
                    occurred_at: occurred,
                    attribution_status: AttributionStatus::Attributed,
                    reverses_consumption_entry_id: Some(original_entry.base.id.clone().into()),
                },
            )?;
            allocations.push(MallRefundAllocation::new(
                MallRefundAllocationId::new(next_id()),
                MallRefundAllocationData {
                    mall_refund_line_id: line.base.id.clone().into(),
                    allocation_no: allocation.allocation_no,
                    original_consumption_entry_id: allocation.original_consumption_entry_id.clone(),
                    original_payment_source_id: allocation.original_payment_source_id.clone(),
                    allocated_refund_amount: amount,
                    allocation_action: AllocationAction::Apply,
                    reverses_allocation_id: None,
                    reversal_consumption_entry_id: Some(reversal.base.id.clone().into()),
                },
            )?);
            reversal_entries.push(reversal);
        }
        for line in &lines {
            line.ensure_allocation_total(&allocations)?;
        }
        Ok((lines, allocations, reversal_entries))
    }

    /// 构建余额恢复分配（解析关联退款头，校验卡实例与累计恢复上限）。
    ///
    /// # 参数
    /// * `req` - 余额恢复事实接收请求
    /// * `restoration_id` - 恢复头 ID
    /// * `after_sales_request_id` - 同一售后案件
    ///
    /// # 返回
    /// 返回 `(恢复分配, 关联退款头 ID)`。
    ///
    /// # 错误
    /// 分配与退款/卡实例不匹配或累计恢复超限时返回 `BusinessLogicError`。
    async fn build_restoration_allocations(
        &self,
        req: &ReceiveBalanceRestorationRequest,
        restoration_id: &MallBalanceRestorationId,
        after_sales_request_id: &MallAfterSalesRequestId,
    ) -> Result<(Vec<MallBalanceRestorationAllocation>, MallRefundId)> {
        let mut allocations = Vec::with_capacity(req.allocations.len());
        let mut refund_id: Option<MallRefundId> = None;
        for allocation in &req.allocations {
            let refund_allocation = self
                .db
                .mall_refund_allocations()
                .find_by_id(&allocation.mall_refund_allocation_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("原退款分配不存在".to_string()))?;
            if !refund_allocation.is_restorable_apply() {
                return Err(Error::BusinessLogicError(
                    "余额恢复只能引用净有效的 APPLY 退款分配".to_string(),
                ));
            }
            let line = self
                .db
                .mall_refund_lines()
                .find_by_id(&refund_allocation.mall_refund_line_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("退款行不存在".to_string()))?;
            if !refund_allocation.belongs_to_line(&MallRefundLineId::new(line.base.id.clone())) {
                return Err(Error::BusinessLogicError(
                    "退款分配与退款行关系不一致".to_string(),
                ));
            }
            let refund = self
                .db
                .mall_refunds()
                .find_by_id(&line.mall_refund_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("退款头不存在".to_string()))?;
            if !refund.belongs_to_after_sales_request(after_sales_request_id) {
                return Err(Error::BusinessLogicError(
                    "退款分配不属于同一售后案件".to_string(),
                ));
            }
            if let Some(expected_refund_id) = &refund_id {
                if !line.belongs_to_refund(expected_refund_id) {
                    return Err(Error::BusinessLogicError(
                        "同一余额恢复不得跨多个退款头分配".to_string(),
                    ));
                }
            } else {
                refund_id = Some(line.mall_refund_id.clone());
            }
            let payment_source: Option<MallPaymentSource> = self
                .db
                .mall_payment_sources()
                .find_by_id(&refund_allocation.original_payment_source_id, &mut NoTransaction)
                .await?;
            let source =
                payment_source.ok_or_else(|| Error::BusinessLogicError("原支付来源不存在".to_string()))?;
            self.db
                .mall_card_instances()
                .find_by_id(&allocation.mall_card_instance_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("恢复卡实例不存在".to_string()))?;
            if !source.uses_card_instance(&allocation.mall_card_instance_id) {
                return Err(Error::BusinessLogicError(
                    "恢复卡实例必须等于原支付来源的卡实例".to_string(),
                ));
            }
            let amount = Amount::from_str(&allocation.restored_amount)?;
            let restored = self
                .restored_for_refund_allocation(&allocation.mall_refund_allocation_id)
                .await?
                .checked_add(amount);
            if !refund_allocation.allows_cumulative_restoration(restored) {
                return Err(Error::BusinessLogicError(
                    "累计恢复金额不得超过对应 CARD 退款净额".to_string(),
                ));
            }
            allocations.push(MallBalanceRestorationAllocation::new(
                MallBalanceRestorationAllocationId::new(next_id()),
                MallBalanceRestorationAllocationData {
                    mall_balance_restoration_id: restoration_id.clone(),
                    allocation_no: allocation.allocation_no,
                    mall_refund_allocation_id: allocation.mall_refund_allocation_id.clone(),
                    mall_card_instance_id: allocation.mall_card_instance_id.clone(),
                    restored_amount: amount,
                },
            )?);
        }
        let refund_id =
            refund_id.ok_or_else(|| Error::BusinessLogicError("余额恢复必须包含至少一条分配".to_string()))?;
        Ok((allocations, refund_id))
    }

    /// 计算原消费已被成功退款覆盖的净额（历史 `APPLY − REVERSE` 合计）。
    ///
    /// # 参数
    /// * `entry_id` - 原消费事实 ID
    ///
    /// # 返回
    /// 返回净退款金额。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn refunded_net_for_entry(&self, entry_id: &MallConsumptionEntryId) -> Result<Amount> {
        let allocations = self
            .db
            .mall_refund_allocations()
            .list_by_consumption(entry_id, &mut NoTransaction)
            .await?;
        let mut net = Amount::from_str("0.00")?;
        for allocation in allocations {
            net = allocation.apply_to_net(net);
        }
        Ok(net)
    }

    /// 计算指定退款分配已被余额恢复覆盖的金额。
    ///
    /// # 参数
    /// * `refund_allocation_id` - 原 CARD 退款分配 ID
    ///
    /// # 返回
    /// 返回已恢复金额。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn restored_for_refund_allocation(
        &self,
        refund_allocation_id: &MallRefundAllocationId,
    ) -> Result<Amount> {
        let allocations = self
            .db
            .mall_balance_restoration_allocations()
            .list_by_refund_allocation(refund_allocation_id, &mut NoTransaction)
            .await?;
        let mut total = Amount::from_str("0.00")?;
        for allocation in allocations {
            total = allocation.add_to_total(total);
        }
        Ok(total)
    }
}

/// 从事实实体构造接收结果视图。
///
/// # 参数
/// * `fact` - 关键事实实体
///
/// # 返回
/// 返回接收结果视图。
fn hit_view(fact: &MallOrderFact) -> ReceivedFactView {
    ReceivedFactView {
        fact_id: fact.base.id.clone(),
        fact_type: fact.fact_type,
        processing_status: fact.processing_status,
        idempotent_hit: true,
    }
}

/// 按白名单字段与方向对退款头排序（`refunded_at` 或 `created_at`）。
///
/// # 参数
/// * `refunds` - 待排序的退款头
/// * `sort_by` - 已过白名单校验的排序字段
/// * `sort_dir` - 排序方向
fn sort_refunds(refunds: &mut [MallRefund], sort_by: &str, sort_dir: SortDir) {
    let ascending = matches!(sort_dir, SortDir::Asc);
    match sort_by {
        "refunded_at" => refunds.sort_by_key(|refund| (refund.refunded_at, refund.base.id.clone())),
        _ => refunds.sort_by_key(|refund| (refund.base.created_at, refund.base.id.clone())),
    }
    if !ascending {
        refunds.reverse();
    }
}

/// 按白名单字段与方向对恢复头排序（`restored_at` 或 `created_at`）。
///
/// # 参数
/// * `restorations` - 待排序的恢复头
/// * `sort_by` - 已过白名单校验的排序字段
/// * `sort_dir` - 排序方向
fn sort_restorations(restorations: &mut [MallBalanceRestoration], sort_by: &str, sort_dir: SortDir) {
    let ascending = matches!(sort_dir, SortDir::Asc);
    match sort_by {
        "restored_at" => {
            restorations.sort_by_key(|restoration| (restoration.restored_at, restoration.base.id.clone()))
        }
        _ => {
            restorations.sort_by_key(|restoration| (restoration.base.created_at, restoration.base.id.clone()))
        }
    }
    if !ascending {
        restorations.reverse();
    }
}

/// 对已排序集合做内存分页切片并映射为视图。
///
/// # 参数
/// * `rows` - 已按白名单排序的全量记录
/// * `paging` - 分页参数
/// * `map` - 实体 → 视图映射
///
/// # 返回
/// 返回 `(当前页视图, 总数)`。
fn slice_page<T, V, F>(rows: Vec<T>, paging: dto::PageParams, map: F) -> (Vec<V>, i64)
where
    F: Fn(T) -> V,
{
    let total = rows.len() as i64;
    let start = ((paging.page.max(1) - 1) * u64::from(paging.page_size)) as usize;
    let items = rows
        .into_iter()
        .skip(start)
        .take(paging.page_size as usize)
        .map(map)
        .collect();
    (items, total)
}
