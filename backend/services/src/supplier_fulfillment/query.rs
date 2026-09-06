use database::{SupplierApiExt, SupplierExt, SupplierFulfillmentExt};
use entities::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use entities::supplier_fulfillment::{
    SupplierFulfillmentOrder, SupplierFulfillmentOrderId, SupplierOrderAction, SupplierOrderActionType,
};
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::dto::{
    PageView, SortDir, SupplierFulfillmentOrderDetailParams, SupplierFulfillmentOrderDetailView,
    SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderView, SupplierOrderActionBlockerView,
    SupplierOrderAddressView, SupplierOrderAllowedAction, SupplierOrderInvestigationEvidenceView,
    SupplierOrderInvestigationOutcome, SupplierRefundFactView,
};
use super::investigate::{
    capability_for_action, ensure_replay_safe, ensure_task_actor_eligible, parse_investigation_evidence,
    verified_terminal_evidence, InvestigationEvidenceRecord,
};
use super::mapping::{item_view, refund_fact_view};
use super::place::ensure_capability;
use super::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use crate::errors::{Error, Result};
use crate::workflow_compose::work_item_service;
use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_workflow::service::work_item::WorkItemAllowedAction;

/// 履约订单列表筛选条件类型（经 `SupplierFulfillmentExt` 关联类型跨 crate 可达）。
type FulfillmentOrderFilter = <mongodb::Database as SupplierFulfillmentExt>::SupplierFulfillmentOrderFilter;

impl SupplierFulfillmentService {
    /// 分页查询供应商履约订单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`supplier_id`/三条状态/`external_order_no` 等扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_list(
        &self,
        params: &SupplierFulfillmentOrderListParams,
    ) -> Result<PageView<SupplierFulfillmentOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = FulfillmentOrderFilter {
            supplier_id: query.supplier_id,
            fulfillment_status: query.fulfillment_status,
            external_order_no: query.external_order_no,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierFulfillmentOrderView {
                id: row.id,
                fulfillment_order_no: row.fulfillment_order_no,
                supplier_id: row.supplier_id.to_string(),
                connection_id: row.connection_id.to_string(),
                split_no: row.split_no,
                fulfillment_status: row.fulfillment_status,
                cancel_status: row.cancel_status,
                refund_status: row.refund_status,
                external_order_no: row.external_order_no,
                submitted_at: row.submitted_at.map(|t| t.unix_secs()),
                accepted_at: row.accepted_at.map(|t| t.unix_secs()),
                completed_at: row.completed_at.map(|t| t.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询供应商履约订单详情（订单 + 明细 + 状态历史 + 动作 + 退款事实）。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_detail(
        &self,
        id: &str,
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        let order = self.load_order(id).await?;
        let order_id = SupplierFulfillmentOrderId::new(id);
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(&order_id), &mut NoTransaction)
            .await?;
        let actions = self
            .db
            .supplier_order_actions()
            .list_by_order_newest(&order_id, &mut NoTransaction)
            .await?;
        let histories = self
            .db
            .supplier_order_status_histories()
            .list_by_order_chronological(&order_id, &mut NoTransaction)
            .await?;
        let refund_views = self.refund_views_for_order(&order_id).await?;

        let supplier_id = order.supplier_id.to_string();
        let supplier_name = self
            .db
            .supplier()
            .current_legal_names_by_account_ids(std::slice::from_ref(&order.supplier_id), &mut NoTransaction)
            .await?
            .remove(&supplier_id);
        let mut action_blockers = Vec::new();
        if supplier_name.is_none() {
            action_blockers.push(supplier_order_blocker(
                "VIEW_SUPPLIER_NAME",
                "SUPPLIER_NAME_MISSING",
                "供应商主体或当前名称修订缺失，禁止以供应商 ID 伪装名称",
            ));
        }
        action_blockers.push(supplier_order_blocker(
            "REVEAL_ADDRESS",
            "ADDRESS_REVEAL_NOT_REGISTERED",
            "当前 W26 尚未注册可审计的短时地址揭示入口",
        ));

        let target_action = actions
            .iter()
            .find(|action| action.action_type != SupplierOrderActionType::Query);
        let latest_investigation = target_action.and_then(|target| {
            actions.iter().find_map(|candidate| {
                let record = parse_investigation_evidence(candidate).ok()?;
                (record.target_supplier_action_id == target.base.id).then_some((candidate, record))
            })
        });
        let target_supplier_action_id = target_action.map(|action| action.base.id.clone());
        let last_investigation = latest_investigation
            .as_ref()
            .map(|(evidence, record)| investigation_evidence_view(&order, evidence, record));

        let work_item_id = params
            .work_item_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let formal = if let Some(work_item_id) = work_item_id {
            let view = work_item_service(self.db.clone(), rbac)
                .authorize_work_item(work_item_id, actor)
                .await?;
            if !matches!(
                view.item.work_item_type,
                WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
            ) || view.item.business_object_type != W26_BUSINESS_OBJECT_TYPE
                || view.item.business_object_id != order.base.id
                || view.item.subject_version != order.base.version.to_string()
                || false
            {
                return Err(Error::BusinessLogicError(
                    "正式任务与当前供应商履约订单不匹配".to_string(),
                ));
            }
            Some(view)
        } else {
            None
        };
        let mut allowed_actions = Vec::new();
        let can_process_formal = formal
            .as_ref()
            .is_some_and(|item| item.allowed_actions.contains(&WorkItemAllowedAction::Process));
        let has_active_task = self
            .db
            .work_items()
            .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, id, &mut NoTransaction)
            .await?
            .into_iter()
            .next()
            .is_some();
        let can_investigate = if formal.is_some() {
            if !can_process_formal {
                block_supplier_order_domain_actions(
                    &mut action_blockers,
                    "CURRENT_RESPONSIBILITY_REQUIRED",
                    "当前账号不是开放任务的当前责任人",
                );
                false
            } else {
                let raw = self
                    .db
                    .work_items()
                    .find_by_id(work_item_id.expect("formal work item id"), &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                if ensure_task_actor_eligible(&self.db, &raw, actor.id(), &mut NoTransaction)
                    .await
                    .is_err()
                {
                    block_supplier_order_domain_actions(
                        &mut action_blockers,
                        "ACTOR_INELIGIBLE",
                        "当前账号已不具备该供应商履约任务的角色或组织资格",
                    );
                    false
                } else {
                    true
                }
            }
        } else if has_active_task {
            block_supplier_order_domain_actions(
                &mut action_blockers,
                "FORMAL_WORK_ITEM_REQUIRED",
                "当前订单存在正式异常任务，必须从该待办携带明确任务身份进入",
            );
            false
        } else {
            true
        };

        if can_investigate {
            if let Some(target) = target_action {
                self.project_supplier_order_actions(
                    &order,
                    target,
                    latest_investigation.as_ref(),
                    formal.is_some(),
                    &mut allowed_actions,
                    &mut action_blockers,
                )
                .await?;
            } else {
                block_supplier_order_domain_actions(
                    &mut action_blockers,
                    "ORIGINAL_SUPPLIER_ACTION_MISSING",
                    "当前订单缺少可调查的原下单、取消或退款动作",
                );
            }
        }

        Ok(SupplierFulfillmentOrderDetailView {
            order: order.into(),
            items: items.into_iter().map(item_view).collect(),
            status_history: histories.into_iter().map(Into::into).collect(),
            actions: actions.into_iter().map(Into::into).collect(),
            refund_facts: refund_views,
            supplier_name,
            address: SupplierOrderAddressView {
                masked: None,
                can_reveal: false,
                blocker_code: Some("ADDRESS_REVEAL_NOT_REGISTERED".to_string()),
                blocker_message: Some("当前 W26 尚未注册可审计的短时地址揭示入口".to_string()),
            },
            work_item: None,
            target_supplier_action_id,
            last_investigation,
            allowed_actions,
            action_blockers,
        })
    }

    /// 以原供应商动作、连接能力和最新结构化证据投影 W26 动作。
    async fn project_supplier_order_actions(
        &self,
        order: &SupplierFulfillmentOrder,
        target: &SupplierOrderAction,
        latest_investigation: Option<&(&SupplierOrderAction, InvestigationEvidenceRecord)>,
        formal_entry: bool,
        allowed_actions: &mut Vec<SupplierOrderAllowedAction>,
        action_blockers: &mut Vec<SupplierOrderActionBlockerView>,
    ) -> Result<()> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?;
        let connection_active = connection.as_ref().is_some_and(SupplierApiConnection::is_active);
        let capabilities = if connection_active {
            self.db
                .supplier_api_capabilities()
                .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
                .await?
        } else {
            Vec::new()
        };

        if order.verified_resolution(target).is_some()
            || (connection_active
                && ensure_capability(&capabilities, SupplierApiCapabilityCode::Query).is_ok())
        {
            allowed_actions.push(SupplierOrderAllowedAction::QueryResult);
        } else {
            action_blockers.push(supplier_order_blocker(
                SupplierOrderAllowedAction::QueryResult.as_str(),
                if connection_active {
                    "QUERY_CAPABILITY_MISSING"
                } else {
                    "SUPPLIER_CONNECTION_UNAVAILABLE"
                },
                if connection_active {
                    "供应商连接未声明启用的结果查询能力"
                } else {
                    "供应商连接不存在或未启用，且当前业务事实尚不能证明终态"
                },
            ));
        }

        let replay_capability_ready = capability_for_action(target.action_type)
            .ok()
            .is_some_and(|needed| ensure_capability(&capabilities, needed).is_ok());
        if connection_active
            && replay_capability_ready
            && ensure_replay_safe(&self.db, order, target, &mut NoTransaction)
                .await
                .is_ok()
        {
            allowed_actions.push(SupplierOrderAllowedAction::Replay);
        } else {
            action_blockers.push(supplier_order_blocker(
                SupplierOrderAllowedAction::Replay.as_str(),
                "VERIFIED_NO_RESULT_REQUIRED",
                "只有最新调查证据明确证明原下单未形成结果，且原能力仍启用时才能重放",
            ));
        }

        let terminal_evidence = latest_investigation.and_then(|(evidence, record)| {
            let resolution = record.verified_resolution?;
            (verified_terminal_evidence(evidence, order, resolution).is_ok()
                && order.verified_resolution(target).map(Into::into) == Some(resolution))
            .then_some(resolution)
        });
        if formal_entry && terminal_evidence.is_some() {
            allowed_actions.push(SupplierOrderAllowedAction::ConfirmVerifiedTerminalResult);
        } else {
            action_blockers.push(supplier_order_blocker(
                SupplierOrderAllowedAction::ConfirmVerifiedTerminalResult.as_str(),
                if formal_entry {
                    "VERIFIED_TERMINAL_EVIDENCE_REQUIRED"
                } else {
                    "FORMAL_WORK_ITEM_REQUIRED"
                },
                if formal_entry {
                    "必须先取得与当前业务事实一致的最新已验证终态证据"
                } else {
                    "确认终态并完成任务只允许从明确的 W26 正式待办入口执行"
                },
            ));
        }
        Ok(())
    }

    /// 按子订单加载全部退款事实视图（含分配行）。
    ///
    /// 归组与排序由 `SupplierFulfillmentRepository::refund_fact_bundles_by_order`
    /// 承担；本方法只把归组快照映射为响应视图。
    ///
    /// # 参数
    /// * `order_id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回退款事实视图集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn refund_views_for_order(
        &self,
        order_id: &SupplierFulfillmentOrderId,
    ) -> Result<Vec<SupplierRefundFactView>> {
        let bundles = self
            .db
            .supplier_fulfillment()
            .refund_fact_bundles_by_order(order_id, &mut NoTransaction)
            .await?;
        Ok(bundles
            .into_iter()
            .map(|bundle| refund_fact_view(&bundle.fact, &bundle.allocations))
            .collect())
    }

    /// 按 ID 加载未删除供应商子订单。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回订单实体。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    pub(super) async fn load_order(&self, id: &str) -> Result<SupplierFulfillmentOrder> {
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))
    }
}

fn supplier_order_blocker(action: &str, code: &str, message: &str) -> SupplierOrderActionBlockerView {
    SupplierOrderActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        destination_workspace_id: None,
    }
}

fn block_supplier_order_domain_actions(
    blockers: &mut Vec<SupplierOrderActionBlockerView>,
    code: &str,
    message: &str,
) {
    for action in [
        SupplierOrderAllowedAction::QueryResult,
        SupplierOrderAllowedAction::Replay,
        SupplierOrderAllowedAction::ConfirmVerifiedTerminalResult,
    ] {
        blockers.push(supplier_order_blocker(action.as_str(), code, message));
    }
}

fn investigation_evidence_view(
    order: &SupplierFulfillmentOrder,
    evidence: &SupplierOrderAction,
    record: &InvestigationEvidenceRecord,
) -> SupplierOrderInvestigationEvidenceView {
    SupplierOrderInvestigationEvidenceView {
        evidence_id: evidence.base.id.clone(),
        target_supplier_action_id: record.target_supplier_action_id.clone(),
        outcome: record.outcome,
        recorded_at: i64::try_from(evidence.base.created_at).unwrap_or(i64::MAX),
        can_safe_retry: record.outcome == SupplierOrderInvestigationOutcome::VerifiedNoResult,
        external_order_no: order.external_order_no.clone(),
        summary: record.summary.clone(),
        verified_supplier_action_result_id: (record.outcome
            == SupplierOrderInvestigationOutcome::VerifiedTerminal)
            .then(|| evidence.base.id.clone()),
        verified_resolution: record.verified_resolution,
    }
}
