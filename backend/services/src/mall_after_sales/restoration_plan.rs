//! 余额恢复分配计划与恢复额度校验（INT-R12）。
//!
//! Repository 批量提供退款关联事实图与历史恢复净额；Entity
//! [`RestorationLimitPlan`] 聚合本请求恢复净额并校验上限；Service 保留
//! same-case、card 归属，并以商城订单版本 CAS 串行化并发占用。

use database::{CardInstanceExt, MallAfterSalesExt, NoTransaction};
use entities::ids::{MallBalanceRestorationAllocationId, MallBalanceRestorationId, MallRefundId};
use entities::mall_after_sales::{
    MallBalanceRestorationAllocation, MallBalanceRestorationAllocationData,
    PendingRestorationRefundAllocation, RestorationLimitPlan,
};
use entities::money::Amount;
use id_generator::next_id;
use std::str::FromStr;

use super::dto::ReceiveBalanceRestorationRequest;
use super::MallAfterSalesService;
use crate::errors::{Error, Result};

impl MallAfterSalesService {
    /// 构建余额恢复分配（解析关联退款图，校验卡实例与累计恢复上限）。
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
    /// 分配与退款/卡实例不匹配或累计恢复超限时返回 `BusinessLogicError`／实体错误。
    ///
    /// # 约束
    /// 额度上限由 Entity 聚合校验；并发写偏斜由调用方在事务内对商城订单做 CAS。
    pub(super) async fn build_restoration_allocations(
        &self,
        req: &ReceiveBalanceRestorationRequest,
        restoration_id: &MallBalanceRestorationId,
        after_sales_request_id: &entities::ids::MallAfterSalesRequestId,
    ) -> Result<(Vec<MallBalanceRestorationAllocation>, MallRefundId)> {
        let allocation_ids: Vec<entities::ids::MallRefundAllocationId> = req
            .allocations
            .iter()
            .map(|allocation| allocation.mall_refund_allocation_id.clone())
            .collect();
        let scope = self
            .db
            .mall_after_sales()
            .restoration_limit_scope(&allocation_ids, &mut NoTransaction)
            .await?;
        let pending = self.pending_restoration_lines(req)?;
        RestorationLimitPlan::validate(&scope.refund_allocations, &scope.historical_restored, &pending)?;

        let mut allocations = Vec::with_capacity(req.allocations.len());
        let mut refund_id: Option<MallRefundId> = None;
        for allocation in &req.allocations {
            let refund_allocation = scope
                .refund_allocations
                .get(&allocation.mall_refund_allocation_id)
                .ok_or_else(|| Error::BusinessLogicError("原退款分配不存在".to_string()))?;
            if !refund_allocation.is_restorable_apply() {
                return Err(Error::BusinessLogicError(
                    "余额恢复只能引用净有效的 APPLY 退款分配".to_string(),
                ));
            }
            let line = scope
                .refund_lines
                .get(&refund_allocation.mall_refund_line_id)
                .ok_or_else(|| Error::BusinessLogicError("退款行不存在".to_string()))?;
            if !refund_allocation.belongs_to_line(&entities::ids::MallRefundLineId::new(line.base.id.clone()))
            {
                return Err(Error::BusinessLogicError(
                    "退款分配与退款行关系不一致".to_string(),
                ));
            }
            let refund = scope
                .refunds
                .get(&line.mall_refund_id)
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
            let source = scope
                .payment_sources
                .get(&refund_allocation.original_payment_source_id)
                .ok_or_else(|| Error::BusinessLogicError("原支付来源不存在".to_string()))?;
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

    /// 将请求恢复分配转换为 Entity 额度行。
    ///
    /// # 参数
    /// * `req` - 余额恢复请求
    ///
    /// # 返回
    /// 返回待校验额度行。
    ///
    /// # 错误
    /// 金额解析失败时返回错误。
    fn pending_restoration_lines(
        &self,
        req: &ReceiveBalanceRestorationRequest,
    ) -> Result<Vec<PendingRestorationRefundAllocation>> {
        pending_restoration_lines(req)
    }
}

/// 在事务会话下重读恢复额度范围并再次校验本请求净额。
///
/// # 参数
/// * `db` - 数据库
/// * `req` - 余额恢复请求
/// * `executor` - 必须与写入位于同一事务会话
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// 原分配缺失或累计超限时返回实体／业务错误。
///
/// # 约束
/// 必须在商城订单 CAS 成功之后调用，避免写偏斜窗口。
pub(super) async fn revalidate_restoration_limits(
    db: &mongodb::Database,
    req: &ReceiveBalanceRestorationRequest,
    executor: &mut dyn database::Executor,
) -> Result<()> {
    use database::MallAfterSalesExt;

    let allocation_ids: Vec<entities::ids::MallRefundAllocationId> = req
        .allocations
        .iter()
        .map(|allocation| allocation.mall_refund_allocation_id.clone())
        .collect();
    let scope = db
        .mall_after_sales()
        .restoration_limit_scope(&allocation_ids, executor)
        .await?;
    let pending = pending_restoration_lines(req)?;
    RestorationLimitPlan::validate(&scope.refund_allocations, &scope.historical_restored, &pending)?;
    Ok(())
}

/// 将请求恢复分配转换为 Entity 额度行。
///
/// # 参数
/// * `req` - 余额恢复请求
///
/// # 返回
/// 返回待校验额度行。
///
/// # 错误
/// 金额解析失败时返回错误。
fn pending_restoration_lines(
    req: &ReceiveBalanceRestorationRequest,
) -> Result<Vec<PendingRestorationRefundAllocation>> {
    req.allocations
        .iter()
        .map(|allocation| {
            Ok(PendingRestorationRefundAllocation {
                mall_refund_allocation_id: allocation.mall_refund_allocation_id.clone(),
                amount: Amount::from_str(&allocation.restored_amount)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{ensure_indexes, CardInstanceExt, MallAfterSalesExt, MallOrderExt, NoTransaction};
    use entities::card_instance::{CardSourceType, MallCardInstance, MallCardInstanceData};
    use entities::common::time::Instant;
    use entities::ids::{
        ExternalIdentityMapId, MallAfterSalesRequestId, MallCardInstanceId, MallConsumptionEntryId,
        MallOrderFactId, MallOrderId, MallOrderItemId, MallPaymentSourceId, MallRefundAllocationId,
        MallRefundId, MallRefundLineId, SalesOrderId, SalesOrderRevisionId,
    };
    use entities::mall_after_sales::{
        AllocationAction, MallRefund, MallRefundAllocation, MallRefundAllocationData, MallRefundData,
        MallRefundLine, MallRefundLineData,
    };
    use entities::mall_order::{
        AttributionStatus, DataSource, MallPaymentSource, MallPaymentSourceData, PaymentSourceType,
    };
    use entities::money::{Amount, Quantity};
    use test_support::{require_mongo, TestDb};

    use crate::mall_after_sales::dto::{ReceiveBalanceRestorationRequest, RestorationAllocationData};

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn card_instance(id: &str, opaque_ref: &str) -> MallCardInstance {
        MallCardInstance::new(
            MallCardInstanceId::new(id),
            MallCardInstanceData {
                mall_id: "mall-a".to_string(),
                opaque_instance_ref: opaque_ref.to_string(),
                origin_sales_order_source_identity_id: ExternalIdentityMapId::new("eim-1"),
                origin_sales_order_id: SalesOrderId::new("so-1"),
                origin_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                source_baseline_version: Some("v1".to_string()),
                initial_balance: amount("100.00"),
                baseline_at: Instant::from_unix_secs(1_700_000_000),
                source_type: CardSourceType::Realtime,
            },
        )
        .expect("卡实例构造失败")
    }

    fn payment_source(id: &str, card_id: &str) -> MallPaymentSource {
        MallPaymentSource::new(
            MallPaymentSourceId::new(id),
            MallPaymentSourceData {
                mall_order_id: MallOrderId::new("order-1"),
                source_no: 1,
                source_type: PaymentSourceType::Card,
                amount: amount("80.00"),
                source_card_instance_ref: Some(format!("ref-{card_id}")),
                mall_card_instance_id: Some(MallCardInstanceId::new(card_id)),
                wechat_payment_ref: None,
                attribution_status: AttributionStatus::Attributed,
            },
        )
        .expect("支付来源构造失败")
    }

    fn refund_header(id: &str, request_id: &str) -> MallRefund {
        MallRefund::new(
            MallRefundId::new(id),
            MallRefundData {
                mall_order_fact_id: MallOrderFactId::new(format!("refund-fact-{id}")),
                after_sales_request_id: MallAfterSalesRequestId::new(request_id),
                mall_id: "mall-a".to_string(),
                external_refund_no: format!("RF-{id}"),
                external_refund_version: "1".to_string(),
                mall_order_id: MallOrderId::new("order-1"),
                refund_amount: amount("80.00"),
                refunded_at: Instant::from_unix_secs(1_700_000_100),
            },
        )
        .expect("退款头构造失败")
    }

    fn refund_line(id: &str, refund_id: &str, line_no: u32) -> MallRefundLine {
        MallRefundLine::new(
            MallRefundLineId::new(id),
            MallRefundLineData {
                mall_refund_id: MallRefundId::new(refund_id),
                line_no,
                mall_order_item_id: MallOrderItemId::new("item-1"),
                refunded_quantity: Quantity::from_str("1.000000").unwrap(),
                line_refund_amount: amount("80.00"),
            },
        )
        .expect("退款行构造失败")
    }

    fn refund_allocation(
        id: &str,
        line_id: &str,
        payment_source_id: &str,
        allocation_no: u32,
    ) -> MallRefundAllocation {
        MallRefundAllocation::new(
            MallRefundAllocationId::new(id),
            MallRefundAllocationData {
                mall_refund_line_id: MallRefundLineId::new(line_id),
                allocation_no,
                original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
                original_payment_source_id: MallPaymentSourceId::new(payment_source_id),
                allocated_refund_amount: amount("80.00"),
                allocation_action: AllocationAction::Apply,
                reverses_allocation_id: None,
                reversal_consumption_entry_id: Some(MallConsumptionEntryId::new(format!("rev-{id}"))),
            },
        )
        .expect("退款分配构造失败")
    }

    fn restoration_request(
        after_sales_request_id: &str,
        allocations: Vec<RestorationAllocationData>,
    ) -> ReceiveBalanceRestorationRequest {
        ReceiveBalanceRestorationRequest {
            mall_id: "mall-a".to_string(),
            source_event_id: "evt-1".to_string(),
            inbox_message_id: "inbox-1".to_string(),
            business_fact_key: "bfk-1".to_string(),
            external_order_no: "SO-1".to_string(),
            external_order_version: "1".to_string(),
            after_sales_request_id: MallAfterSalesRequestId::new(after_sales_request_id),
            original_payment_fact_id: MallOrderFactId::new("pay-1"),
            occurred_at: 1_700_000_200,
            received_at: 1_700_000_201,
            data_source: DataSource::Realtime,
            raw_payload_reference: None,
            external_restoration_no: "BR-1".to_string(),
            version: "1".to_string(),
            restored_amount: "10.00".to_string(),
            restored_at: 1_700_000_202,
            allocations,
        }
    }

    /// 错误卡实例：卡存在但不属于原支付来源，计划失败且不产生恢复分配写入。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn card_mismatch_rejects_without_persistence() {
        require_mongo!(async {
            let fixture = TestDb::new("int_r12_card_mismatch")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();

            db.mall_card_instances()
                .create(&card_instance("card-1", "ref-card-1"), &mut NoTransaction)
                .await
                .expect("原卡写入失败");
            db.mall_card_instances()
                .create(&card_instance("card-wrong", "ref-card-wrong"), &mut NoTransaction)
                .await
                .expect("错误卡写入失败");
            db.mall_payment_sources()
                .create(&payment_source("ps-1", "card-1"), &mut NoTransaction)
                .await
                .expect("支付来源写入失败");
            db.mall_refunds()
                .create(&refund_header("refund-1", "asr-1"), &mut NoTransaction)
                .await
                .expect("退款头写入失败");
            db.mall_refund_lines()
                .create(&refund_line("rl-1", "refund-1", 1), &mut NoTransaction)
                .await
                .expect("退款行写入失败");
            db.mall_refund_allocations()
                .create(&refund_allocation("ra-1", "rl-1", "ps-1", 1), &mut NoTransaction)
                .await
                .expect("退款分配写入失败");

            let service = MallAfterSalesService::new(db.clone());
            let req = restoration_request(
                "asr-1",
                vec![RestorationAllocationData {
                    allocation_no: 1,
                    mall_refund_allocation_id: MallRefundAllocationId::new("ra-1"),
                    mall_card_instance_id: MallCardInstanceId::new("card-wrong"),
                    restored_amount: "10.00".to_string(),
                }],
            );
            let err = service
                .build_restoration_allocations(
                    &req,
                    &MallBalanceRestorationId::new("br-plan"),
                    &MallAfterSalesRequestId::new("asr-1"),
                )
                .await
                .expect_err("错误卡必须失败");
            assert!(
                err.to_string().contains("恢复卡实例必须等于原支付来源的卡实例"),
                "实际错误: {err}"
            );
            let persisted = db
                .mall_balance_restoration_allocations()
                .list_by_refund_allocation(&MallRefundAllocationId::new("ra-1"), &mut NoTransaction)
                .await
                .expect("恢复分配查询失败");
            assert!(persisted.is_empty(), "失败路径不得写入恢复分配");
        });
    }

    /// 同一恢复请求跨两个退款头：失败关闭且零写入。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn cross_refund_header_rejects_without_persistence() {
        require_mongo!(async {
            let fixture = TestDb::new("int_r12_cross_refund")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();

            db.mall_card_instances()
                .create(&card_instance("card-1", "ref-card-1"), &mut NoTransaction)
                .await
                .expect("卡写入失败");
            db.mall_payment_sources()
                .create(&payment_source("ps-1", "card-1"), &mut NoTransaction)
                .await
                .expect("支付来源写入失败");
            for (refund_id, line_id, ra_id, no) in [
                ("refund-1", "rl-1", "ra-1", 1u32),
                ("refund-2", "rl-2", "ra-2", 1u32),
            ] {
                db.mall_refunds()
                    .create(&refund_header(refund_id, "asr-1"), &mut NoTransaction)
                    .await
                    .expect("退款头写入失败");
                db.mall_refund_lines()
                    .create(&refund_line(line_id, refund_id, no), &mut NoTransaction)
                    .await
                    .expect("退款行写入失败");
                db.mall_refund_allocations()
                    .create(&refund_allocation(ra_id, line_id, "ps-1", no), &mut NoTransaction)
                    .await
                    .expect("退款分配写入失败");
            }

            let service = MallAfterSalesService::new(db.clone());
            let req = restoration_request(
                "asr-1",
                vec![
                    RestorationAllocationData {
                        allocation_no: 1,
                        mall_refund_allocation_id: MallRefundAllocationId::new("ra-1"),
                        mall_card_instance_id: MallCardInstanceId::new("card-1"),
                        restored_amount: "10.00".to_string(),
                    },
                    RestorationAllocationData {
                        allocation_no: 2,
                        mall_refund_allocation_id: MallRefundAllocationId::new("ra-2"),
                        mall_card_instance_id: MallCardInstanceId::new("card-1"),
                        restored_amount: "10.00".to_string(),
                    },
                ],
            );
            let err = service
                .build_restoration_allocations(
                    &req,
                    &MallBalanceRestorationId::new("br-plan"),
                    &MallAfterSalesRequestId::new("asr-1"),
                )
                .await
                .expect_err("跨退款头必须失败");
            assert!(
                err.to_string().contains("同一余额恢复不得跨多个退款头分配"),
                "实际错误: {err}"
            );
            for ra in ["ra-1", "ra-2"] {
                let persisted = db
                    .mall_balance_restoration_allocations()
                    .list_by_refund_allocation(&MallRefundAllocationId::new(ra), &mut NoTransaction)
                    .await
                    .expect("恢复分配查询失败");
                assert!(persisted.is_empty(), "跨退款失败不得写入恢复分配");
            }
        });
    }

    /// 退款头不属于请求售后案件：same-case 失败且零写入。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn after_sales_request_mismatch_rejects_without_persistence() {
        require_mongo!(async {
            let fixture = TestDb::new("int_r12_same_case")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();

            db.mall_card_instances()
                .create(&card_instance("card-1", "ref-card-1"), &mut NoTransaction)
                .await
                .expect("卡写入失败");
            db.mall_payment_sources()
                .create(&payment_source("ps-1", "card-1"), &mut NoTransaction)
                .await
                .expect("支付来源写入失败");
            db.mall_refunds()
                .create(&refund_header("refund-1", "asr-other"), &mut NoTransaction)
                .await
                .expect("退款头写入失败");
            db.mall_refund_lines()
                .create(&refund_line("rl-1", "refund-1", 1), &mut NoTransaction)
                .await
                .expect("退款行写入失败");
            db.mall_refund_allocations()
                .create(&refund_allocation("ra-1", "rl-1", "ps-1", 1), &mut NoTransaction)
                .await
                .expect("退款分配写入失败");

            let service = MallAfterSalesService::new(db.clone());
            let req = restoration_request(
                "asr-1",
                vec![RestorationAllocationData {
                    allocation_no: 1,
                    mall_refund_allocation_id: MallRefundAllocationId::new("ra-1"),
                    mall_card_instance_id: MallCardInstanceId::new("card-1"),
                    restored_amount: "10.00".to_string(),
                }],
            );
            let err = service
                .build_restoration_allocations(
                    &req,
                    &MallBalanceRestorationId::new("br-plan"),
                    &MallAfterSalesRequestId::new("asr-1"),
                )
                .await
                .expect_err("跨售后案件必须失败");
            assert!(
                err.to_string().contains("退款分配不属于同一售后案件"),
                "实际错误: {err}"
            );
            let persisted = db
                .mall_balance_restoration_allocations()
                .list_by_refund_allocation(&MallRefundAllocationId::new("ra-1"), &mut NoTransaction)
                .await
                .expect("恢复分配查询失败");
            assert!(persisted.is_empty(), "same-case 失败不得写入恢复分配");
        });
    }
}
