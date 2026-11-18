use std::future::Future;

use application_core::AuditActor;
use erp_inventory::{
    DocumentApprovalView, InventoryService, Result as InventoryResult, StockAdjustment,
    StockAdjustmentDetailView, StockAdjustmentLineView, StockAdjustmentView,
};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use persistence_core::{Executor, Transactional};

use super::InventoryAdjustmentReadService;
use super::approval::{self, load_approval_binding};
use crate::{Error, Result};

impl InventoryAdjustmentReadService {
    /// 查询库存调整单详情（表头 + 明细 + 过账流水）。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `actor` - 当前认证操作人，用于投影 actor-aware 审批动作
    ///
    /// # 返回
    /// 返回调整单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_detail",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_detail")
    )]
    pub async fn stock_adjustment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        self.stock_adjustment_detail_for_instance(id, actor, None).await
    }

    /// 构造库存调整详情；签署命令结果必须按收据引用的实例精确投影。
    ///
    /// # 参数
    /// * `id` - 调整单主键。
    /// * `actor` - 当前认证操作人，用于投影审批动作。
    /// * `approval_instance` - 为 `Some` 时按该实例与期望主题版本投影；`None` 时加载主题上的当前审批。
    ///
    /// # 返回
    /// 返回含审批、表头、明细与过账流水的详情；姓名和 SKU 名称查询失败时保留缺失名称。
    ///
    /// # 错误
    /// 调整单不可读、绑定缺失、审批投影失败，或明细、流水、责任人员事实读取失败时返回对应错误。
    pub async fn stock_adjustment_detail_for_instance(
        &self,
        id: &str,
        actor: &AuditActor,
        approval_instance: Option<(&str, u32)>,
    ) -> Result<StockAdjustmentDetailView> {
        let service = self.clone();
        let id = id.to_string();
        let read_actor = actor.clone();
        let approval_instance = approval_instance.map(|(id, version)| (id.to_string(), version));
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .detail(
                            &id,
                            &read_actor,
                            approval_instance.as_ref().map(|(id, version)| (id.as_str(), *version)),
                            executor,
                        )
                        .await
                })
            })
            .await
    }

    /// 在调用方读取快照内组装已授权详情，不开启事务。
    ///
    /// # 参数
    /// * `id` - 调整单主键。
    /// * `actor` - 当前认证操作人。
    /// * `approval_instance` - 原命令精确实例与主题版本；空时取当前实例。
    /// * `executor` - 同一授权、业务、审批与人员身份事实读取快照。
    ///
    /// # 返回
    /// 返回同一快照的业务事实、人员身份、展示名称及当前调用人审批动作。
    ///
    /// # 错误
    /// 对象不可读、审批绑定或实例不一致、事实读取失败时返回对应错误。
    pub async fn detail(
        &self,
        id: &str,
        actor: &AuditActor,
        approval_instance: Option<(&str, u32)>,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustmentDetailView> {
        let inventory = &self.inventory;
        let adjustment = inventory.readable_stock_adjustment(id, actor, executor).await?;
        let lines = inventory.load_adjustment_lines(&adjustment.base.id, executor).await?;
        let movements = inventory.load_movements_for_source_document(id, executor).await?;

        let binding = load_approval_binding(&self.db, id, executor).await?;
        let binding = binding.as_ref().ok_or_else(|| {
            Error::ConflictError("这张库存调整单没有可用的审批流程，请联系管理员核对流程设置后再提交".into())
        })?;
        let approval = self.approval_view(&adjustment, binding, actor, approval_instance, executor).await?;

        let mut adjustment_view = StockAdjustmentView::from(adjustment);
        if let Some(fact) = inventory.adjustment_people_by_ids(&[id.to_string()], executor).await?.remove(id)
        {
            adjustment_view.apply_people(&fact);
        }

        let lines = lines.into_iter().map(Into::into).collect::<Vec<StockAdjustmentLineView>>();
        let mut view = StockAdjustmentDetailView {
            approval,
            adjustment: adjustment_view,
            lines,
            posted_movements: movements.into_iter().map(Into::into).collect(),
        };

        enrich_display_names(inventory, &mut view.adjustment, &mut view.lines, actor, executor).await;
        Ok(view)
    }

    async fn approval_view(
        &self,
        adjustment: &StockAdjustment,
        binding: &ApprovalDefinitionBinding,
        actor: &AuditActor,
        approval_instance: Option<(&str, u32)>,
        executor: &mut dyn Executor,
    ) -> Result<DocumentApprovalView> {
        match approval_instance {
            Some((instance_id, expected_subject_version)) => {
                approval::load_document_approval_for_instance(
                    self,
                    adjustment,
                    Some(binding),
                    instance_id,
                    expected_subject_version,
                    actor,
                    executor,
                )
                .await
            },
            None => approval::load_document_approval(self, adjustment, Some(binding), actor, executor).await,
        }
    }
}

/// 可选展示字段与核心事实使用同一执行器；数据库事务失败仍由原事务向上传播。
async fn enrich_display_names(
    inventory: &InventoryService,
    adjustment: &mut StockAdjustmentView,
    lines: &mut [StockAdjustmentLineView],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) {
    optional_display_names(
        inventory.enrich_adjustment_people_names(std::slice::from_mut(adjustment), executor),
        actor,
        "people",
    )
    .await;
    optional_display_names(inventory.enrich_adjustment_line_names(lines, executor), actor, "sku").await;
}

/// 纯展示关联不可用时保留缺失名称；正式命令和授权事实不进入该降级路径。
async fn optional_display_names(
    read: impl Future<Output = InventoryResult<()>>,
    actor: &AuditActor,
    field: &'static str,
) -> bool {
    if read.await.is_ok() {
        return true;
    }
    tracing::warn!(
        account = actor.account(),
        actor_id = actor.id(),
        request_id = actor.request_id().unwrap_or_default(),
        display_field = field,
        "库存调整展示名称读取失败，保留名称缺失占位"
    );
    false
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::future::ready;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_inventory::{
        AdjustmentPeopleFact, AdjustmentPeopleFactsPort, AdjustmentReasonType, AuthorizationPort,
        CatalogFactsPort, Error as InventoryError, FailClosedAuditPort, FailClosedFulfillmentFacts,
        FailClosedWarehouseFacts, InventoryAction, InventoryAuthorization, MovementDirection, SkuFact,
        SkuRevisionFact, StockAdjustmentState,
    };
    use mongodb::Client;
    use mongodb::error::Error as MongoError;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{Error as PersistenceError, ExecutionBoundary};

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("warehouse-user".into(), "cangchu".into(), AccountKind::Admin)
            .with_request_id(Some("request-1".into()))
            .unwrap()
    }

    fn line() -> StockAdjustmentLineView {
        StockAdjustmentLineView {
            id: "line-1".into(),
            sku_id: "sku-1".into(),
            sku_code: None,
            sku_name: None,
            quantity: "2".parse().unwrap(),
            direction: MovementDirection::Increase,
        }
    }

    struct DisplayFacts {
        boundary: ExecutionBoundary,
        fail_people: bool,
        calls: Mutex<Vec<&'static str>>,
    }

    impl DisplayFacts {
        fn record(&self, field: &'static str, executor: &mut dyn Executor) {
            assert!(self.boundary.is_current(executor));
            assert!(executor.boundary().unwrap().is_snapshot());
            self.calls.lock().unwrap().push(field);
        }
    }

    #[async_trait]
    impl AuthorizationPort for DisplayFacts {
        async fn require_adjustment_update(
            &self,
            _: &AuditActor,
            _: &str,
            _: &mut dyn Executor,
        ) -> InventoryResult<()> {
            panic!("展示补齐不得检查修改资格")
        }

        async fn require_adjustment(
            &self,
            _: &AuditActor,
            _: &str,
            _: &mut dyn Executor,
        ) -> InventoryResult<()> {
            panic!("展示补齐不得重新授权")
        }
        async fn authorize(
            &self,
            _: &AuditActor,
            _: InventoryAction,
            _: &mut dyn Executor,
        ) -> InventoryResult<InventoryAuthorization> {
            panic!("展示补齐不得重新授权")
        }
        async fn require_warehouse(
            &self,
            _: &AuditActor,
            _: InventoryAction,
            _: &str,
            _: &mut dyn Executor,
        ) -> InventoryResult<()> {
            panic!("展示补齐不得重新授权")
        }
        async fn allowed_warehouses(
            &self,
            _: &AuditActor,
            _: InventoryAction,
            _: &[String],
            _: &mut dyn Executor,
        ) -> InventoryResult<BTreeSet<String>> {
            panic!("展示补齐不得重新授权")
        }
    }

    #[async_trait]
    impl AdjustmentPeopleFactsPort for DisplayFacts {
        async fn names_by_ids(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> InventoryResult<HashMap<String, String>> {
            self.record("people", executor);
            assert_eq!(ids, ["warehouse-user"]);
            if self.fail_people {
                return Err(InventoryError::Internal("optional-name-unavailable".into()));
            }
            Ok([("warehouse-user".into(), "仓储经办".into())].into())
        }
        async fn people_by_adjustment_ids(
            &self,
            _: &[String],
            _: &mut dyn Executor,
        ) -> InventoryResult<HashMap<String, AdjustmentPeopleFact>> {
            panic!("展示补齐不得重读人员责任事实")
        }
        async fn adjustment_ids_submitted_by(
            &self,
            _: &[String],
            _: &mut dyn Executor,
        ) -> InventoryResult<Vec<String>> {
            panic!("展示补齐不得按人员筛选")
        }
        async fn adjustment_ids_assigned_to(
            &self,
            _: &[String],
            _: &mut dyn Executor,
        ) -> InventoryResult<Vec<String>> {
            panic!("展示补齐不得按人员筛选")
        }
    }

    #[async_trait]
    impl CatalogFactsPort for DisplayFacts {
        async fn skus_by_ids(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> InventoryResult<HashMap<String, SkuFact>> {
            self.record("sku", executor);
            assert_eq!(ids, ["sku-1"]);
            Ok([(
                "sku-1".into(),
                SkuFact {
                    id: "sku-1".into(),
                    sku_no: "SKU-001".into(),
                    current_revision_id: Some("revision".into()),
                },
            )]
            .into())
        }
        async fn sku_revisions_by_ids(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> InventoryResult<HashMap<String, SkuRevisionFact>> {
            self.record("revision", executor);
            assert_eq!(ids, ["revision"]);
            Ok([(
                "revision".into(),
                SkuRevisionFact { id: "revision".into(), name: "礼品卡".into(), specification: None },
            )]
            .into())
        }
    }

    #[tokio::test]
    async fn display_people_and_sku_reads_keep_the_original_snapshot_even_after_optional_failure() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("inventory_display_unit");
        for fail_people in [false, true] {
            let mut executor = SnapshotExecutor::default();
            let facts = Arc::new(DisplayFacts {
                boundary: executor.boundary().unwrap(),
                fail_people,
                calls: Mutex::new(vec![]),
            });
            let inventory = InventoryService::new(
                db.clone(),
                facts.clone(),
                Arc::new(FailClosedWarehouseFacts),
                facts.clone(),
                Arc::new(FailClosedFulfillmentFacts),
                Arc::new(FailClosedAuditPort),
                facts.clone(),
            );

            let mut adjustment = StockAdjustmentView {
                id: "adjustment".into(),
                adjustment_no: "ADJ-1".into(),
                warehouse_id: "warehouse".into(),
                reason_type: AdjustmentReasonType::StockGain,
                status: StockAdjustmentState::Draft,
                prepared_by: "warehouse-user".into(),
                prepared_by_name: None,
                submitted_by: None,
                submitted_by_name: None,
                current_assignee: None,
                current_assignee_name: None,
                reviewed_by: None,
                finance_reviewed_by: None,
                note: None,
                occurred_at: None,
                version: "1".into(),
                created_at: 1,
            };
            let mut lines = vec![line()];

            enrich_display_names(&inventory, &mut adjustment, &mut lines, &actor(), &mut executor).await;
            assert_eq!(facts.calls.lock().unwrap().as_slice(), ["people", "sku", "revision"]);
            assert_eq!(adjustment.prepared_by_name.as_deref(), (!fail_people).then_some("仓储经办"));
            assert_eq!(lines[0].sku_name.as_deref(), Some("礼品卡"));
            assert_eq!(lines[0].quantity, "2".parse().unwrap());
            assert!(facts.boundary.is_current(&executor));
        }
    }

    #[tokio::test]
    async fn optional_name_failure_preserves_business_payload_and_allows_next_read() {
        let actor = actor();
        let mut line = line();
        let original = line.clone();
        let failed = ready(Err(InventoryError::Internal("名称查询不可用".into())));
        assert!(!optional_display_names(failed, &actor, "people").await);
        assert_eq!(line, original);

        let succeeded = async {
            line.sku_code = Some("SKU-001".into());
            line.sku_name = Some("礼品卡".into());
            Ok(())
        };
        assert!(optional_display_names(succeeded, &actor, "sku").await);
        assert_eq!(line.sku_id, original.sku_id);
        assert_eq!(line.quantity, original.quantity);
        assert_eq!(line.sku_name.as_deref(), Some("礼品卡"));
    }

    #[tokio::test]
    async fn optional_sku_name_failure_keeps_names_missing_without_command_error() {
        let actor = actor();
        let line = line();
        let original = line.clone();
        let failed = ready(Err(InventoryError::RepositoryError(PersistenceError::DatabaseError(
            MongoError::custom("商品名称查询不可用"),
        ))));
        assert!(!optional_display_names(failed, &actor, "sku").await);
        assert_eq!(line, original);
        assert!(line.sku_code.is_none());
        assert!(line.sku_name.is_none());
    }
}
