//! 供应商履约详情的跨域只读装配，包括主体名称与 W26 正式任务授权。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AuthorizationService, CompiledAccess};
use erp_permissions::supplier_fulfillment;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::after_sales::{RefundExecutionView, refunds_for_order};
use erp_supply::api_order::ApiOrderExt;
use erp_supply::authorization::CompiledScope;
use erp_supply::dto::supplier_fulfillment::{
    SupplierFulfillmentOrderDetailParams, SupplierOrderActionBlockerView, SupplierOrderAddressView,
    SupplierOrderAllowedAction, SupplierOrderInvestigationEvidenceView, SupplierOrderInvestigationOutcome,
    SupplierRefundFactView,
};
use erp_supply::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use erp_supply::entity::supplier_fulfillment::{
    SupplierFulfillmentItem, SupplierFulfillmentOrder, SupplierFulfillmentOrderId, SupplierOrderAction,
    SupplierOrderActionType, SupplierOrderStatusHistory,
};
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::repository::prelude::*;
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::work_item::WorkItemAllowedAction;
use persistence_core::{Executor, Transactional};

use crate::ports::work_item_authorization::{AuthorizedTaskFact, WorkItemAuthorizationReadPort};
use crate::purchase_center::repository::supplier_names::current_legal_names_by_account_ids;
use crate::{Error, Result};

/// 组合履约订单、主体名称、调查证据和正式任务授权的详情读取器。
#[derive(Clone)]
pub struct SupplierFulfillmentDetailReadService {
    pub(super) db: mongodb::Database,
    fulfillment: Arc<SupplierFulfillmentService>,
    authorization: Arc<AuthorizationService>,
}

struct OrderDetailFacts {
    items: Vec<SupplierFulfillmentItem>,
    actions: Vec<SupplierOrderAction>,
    histories: Vec<SupplierOrderStatusHistory>,
    refunds: Vec<SupplierRefundFactView>,
}

struct OrderActionProjection {
    target_supplier_action_id: Option<String>,
    last_investigation: Option<SupplierOrderInvestigationEvidenceView>,
    allowed_actions: Vec<SupplierOrderAllowedAction>,
}

impl OrderActionProjection {
    /// 自动采购订单只使用持久化执行器，不开放旧网关的调查重放和结果确认。
    fn automatic(blockers: &mut Vec<SupplierOrderActionBlockerView>) -> Self {
        block_supplier_order_domain_actions(
            blockers,
            "AUTOMATIC_API_EXECUTION",
            "此订单由采购 API 流程自动执行和补查，请使用接口配送资料或来源采购处理",
        );
        Self { target_supplier_action_id: None, last_investigation: None, allowed_actions: Vec::new() }
    }
}

struct InvestigationAccess {
    can_investigate: bool,
    formal_entry: bool,
}

/// 两种固定入口保留各自原查询凭证；订单凭证由拥有领域验证模型与读取边界。
enum DetailReadAccess {
    Task(Box<CompiledAccess>),
    Order(CompiledScope),
}

impl DetailReadAccess {
    /// 所有展示补齐结束后验证原入口凭证，不重新开始授权阶段。
    async fn validate(
        &self,
        authorization: &AuthorizationService,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match self {
            Self::Task(access) => authorization.validate(access, executor).await?,
            Self::Order(scope) => scope.validate(executor).await?,
        }
        Ok(())
    }
}

impl SupplierFulfillmentDetailReadService {
    /// 使用同一数据库和组合根已配置的履约服务创建详情读取器。
    ///
    /// 履约服务负责现有单域订单读取规则；详情读取不调用供应商网关。
    ///
    /// # 参数
    /// * `db` - 数据库。
    /// * `fulfillment` - 组合根已配置的履约读取服务。
    /// * `authorization` - 与任务授权端口共用的统一授权服务。
    ///
    /// # 返回
    /// 返回详情读取器。构造不执行查询。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: mongodb::Database,
        fulfillment: SupplierFulfillmentService,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, fulfillment: Arc::new(fulfillment), authorization }
    }

    /// 查询供应商履约订单详情（订单 + 明细 + 状态历史 + 动作 + 退款事实）。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID。
    /// * `params` - 详情参数；非空 `work_item_id` 才进入正式任务授权。
    /// * `actor` - 已认证操作人，用于任务授权。
    /// * `task_auth` - 正式任务读取授权端口。
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在，或已授权的正式任务不存在
    /// * `BusinessLogicError` - 正式任务与当前订单不匹配
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// 任务授权失败时返回对应错误。
    ///
    pub async fn supplier_fulfillment_order_detail(
        &self,
        id: &str,
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        task_auth: Arc<dyn WorkItemAuthorizationReadPort>,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        let (this, id, params, actor) = (self.clone(), id.to_owned(), params.clone(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.detail_snapshot(&id, &params, &actor, task_auth.as_ref(), executor).await
                })
            })
            .await
    }

    /// 先取得当前入口的唯一凭证，再读取全部履约资料，最后交付原凭证。
    async fn detail_snapshot(
        &self,
        id: &str,
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        task_auth: &dyn WorkItemAuthorizationReadPort,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("供应商履约详情读取必须使用快照事务".into()));
        }
        let (order, formal, access) =
            self.detail_authorization(id, params, actor, task_auth, executor).await?;
        let view = self.detail_view(order, formal.as_ref(), executor).await?;
        access.validate(&self.authorization, executor).await?;
        Ok(view)
    }

    /// 正式任务沿工作流固定来源资格读取；普通入口使用订单完整对象查询。
    async fn detail_authorization(
        &self,
        id: &str,
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        task_auth: &dyn WorkItemAuthorizationReadPort,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierFulfillmentOrder, Option<AuthorizedTaskFact>, DetailReadAccess)> {
        if let Some(task_id) = params.work_item_id.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
            let (formal, access) = task_auth.authorize(task_id, actor, executor).await?;
            ensure_task_target(&formal, id)?;
            let order = self
                .db
                .supplier_fulfillment_orders()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("供应商履约订单不存在或无权查看".into()))?;
            if formal.subject_version != order.base.version.to_string() {
                return Err(Error::BusinessLogicError("正式任务与当前供应商履约订单不匹配".into()));
            }
            return Ok((order, Some(formal), DetailReadAccess::Task(Box::new(access))));
        }
        let scope = FulfillmentOrderAccess::new(self.db.clone(), self.authorization.clone())
            .resolve(actor, &supplier_fulfillment::ORDER_DETAIL, executor)
            .await?;
        let order = self
            .db
            .supplier_fulfillment_orders()
            .find_authorized(id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约订单不存在或无权查看".into()))?;
        Ok((order, None, DetailReadAccess::Order(scope)))
    }

    /// 全部名称、退款、调查和物流事实沿当前入口的原执行器补齐。
    async fn detail_view(
        &self,
        order: SupplierFulfillmentOrder,
        formal: Option<&AuthorizedTaskFact>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        let id = order.base.id.as_str();
        let facts = self.order_detail_facts(&SupplierFulfillmentOrderId::new(id), executor).await?;
        let supplier_name =
            current_legal_names_by_account_ids(&self.db, std::slice::from_ref(&order.supplier_id), executor)
                .await?
                .remove(order.supplier_id.as_ref());
        let mut action_blockers = display_blockers(supplier_name.is_some());
        let projection = self
            .order_action_projection(&order, &facts.actions, formal, &mut action_blockers, executor)
            .await?;
        let display_items = self.item_display(facts.items, executor).await?;
        let connection_code = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, executor)
            .await?
            .map(|connection| connection.connection_code);
        let (tracking_error, purchase_order_id, purchase_order_no) =
            self.api_tracking_context(id, executor).await?;
        let api_refunds = refunds_for_order(&self.db, id, executor)
            .await?
            .into_iter()
            .map(RefundExecutionView::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(SupplierFulfillmentOrderDetailView {
            api_refunds,
            order: order.into(),
            items: display_items,
            status_history: facts.histories.into_iter().map(Into::into).collect(),
            actions: facts.actions.into_iter().map(Into::into).collect(),
            refund_facts: facts.refunds,
            supplier_name,
            connection_code,
            address: SupplierOrderAddressView {
                masked: None,
                can_reveal: false,
                blocker_code: Some("ADDRESS_REVEAL_NOT_REGISTERED".to_string()),
                blocker_message: Some("当前 W26 尚未注册可审计的短时地址揭示入口".to_string()),
            },
            work_item: None,
            target_supplier_action_id: projection.target_supplier_action_id,
            last_investigation: projection.last_investigation,
            allowed_actions: projection.allowed_actions,
            action_blockers,
            tracking_error,
            purchase_order_id,
            purchase_order_no,
        })
    }

    async fn api_tracking_context(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Option<String>, Option<String>, Option<String>)> {
        let tracking_error = self
            .db
            .supplier_order_tracking()
            .find_by_id(id, executor)
            .await?
            .and_then(|tracking| tracking.last_error);
        let purchase_order = if let Some(execution) =
            self.db.supplier_api_order_executions().find_by_id(id, executor).await?
        {
            self.db.purchase_orders().find_by_id(&execution.purchase_order_id, executor).await?
        } else {
            None
        };
        let purchase_order_id = purchase_order.as_ref().map(|order| order.base.id.clone());
        let purchase_order_no = purchase_order.map(|order| order.purchase_no);
        Ok((tracking_error, purchase_order_id, purchase_order_no))
    }

    async fn order_detail_facts(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<OrderDetailFacts> {
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(order_id), executor)
            .await?;
        let actions = self.db.supplier_order_actions().list_by_order_newest(order_id, executor).await?;
        let histories =
            self.db.supplier_order_status_histories().list_by_order_chronological(order_id, executor).await?;
        let refunds = self.refund_views_for_order(order_id, executor).await?;
        Ok(OrderDetailFacts { items, actions, histories, refunds })
    }

    async fn order_action_projection(
        &self,
        order: &SupplierFulfillmentOrder,
        actions: &[SupplierOrderAction],
        formal: Option<&AuthorizedTaskFact>,
        blockers: &mut Vec<SupplierOrderActionBlockerView>,
        executor: &mut dyn Executor,
    ) -> Result<OrderActionProjection> {
        if self.db.supplier_api_order_executions().find_by_id(&order.base.id, executor).await?.is_some() {
            return Ok(OrderActionProjection::automatic(blockers));
        }
        let target = actions.iter().find(|action| action.action_type != SupplierOrderActionType::Query);
        let latest = target.and_then(|target| {
            actions.iter().find_map(|candidate| {
                let record = parse_investigation_evidence(candidate).ok()?;
                (record.target_supplier_action_id() == target.base.id).then_some((candidate, record))
            })
        });
        let access = self.investigation_access(order, formal, blockers, executor).await?;
        let mut allowed_actions = Vec::new();
        if access.can_investigate {
            if let Some(target) = target {
                self.project_supplier_order_actions(
                    order,
                    target,
                    latest.as_ref(),
                    access.formal_entry,
                    (&mut allowed_actions, blockers),
                    executor,
                )
                .await?;
            } else {
                block_supplier_order_domain_actions(
                    blockers,
                    "ORIGINAL_SUPPLIER_ACTION_MISSING",
                    "当前订单缺少可调查的原下单、取消或退款动作",
                );
            }
        }
        Ok(OrderActionProjection {
            target_supplier_action_id: target.map(|action| action.base.id.clone()),
            last_investigation: latest
                .as_ref()
                .map(|(evidence, record)| investigation_evidence_view(order, evidence, record)),
            allowed_actions,
        })
    }

    async fn investigation_access(
        &self,
        order: &SupplierFulfillmentOrder,
        formal: Option<&AuthorizedTaskFact>,
        blockers: &mut Vec<SupplierOrderActionBlockerView>,
        executor: &mut dyn Executor,
    ) -> Result<InvestigationAccess> {
        let has_active_task = self
            .db
            .work_items()
            .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, &order.base.id, executor)
            .await?
            .into_iter()
            .next()
            .is_some();
        let can_investigate = if let Some(formal) = formal {
            if !formal.allowed_actions.contains(&WorkItemAllowedAction::Process) {
                block_supplier_order_domain_actions(
                    blockers,
                    "CURRENT_RESPONSIBILITY_REQUIRED",
                    "当前账号不是开放任务的当前责任人",
                );
                false
            } else {
                true
            }
        } else if has_active_task {
            block_supplier_order_domain_actions(
                blockers,
                "FORMAL_WORK_ITEM_REQUIRED",
                "当前订单存在正式异常任务，必须从该待办携带明确任务身份进入",
            );
            false
        } else {
            true
        };
        Ok(InvestigationAccess { can_investigate, formal_entry: formal.is_some() })
    }

    /// 以原供应商动作、连接能力和最新结构化证据投影 W26 动作。
    async fn project_supplier_order_actions(
        &self,
        order: &SupplierFulfillmentOrder,
        target: &SupplierOrderAction,
        latest_investigation: Option<&(&SupplierOrderAction, InvestigationEvidenceRecord)>,
        formal_entry: bool,
        (allowed_actions, action_blockers): (
            &mut Vec<SupplierOrderAllowedAction>,
            &mut Vec<SupplierOrderActionBlockerView>,
        ),
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (connection_active, capabilities) =
            self.project_query_action(order, target, allowed_actions, action_blockers, executor).await?;

        let replay_capability_ready = capability_for_action(target.action_type)
            .ok()
            .is_some_and(|needed| ensure_capability(&capabilities, needed).is_ok());
        if (connection_active || target.action_type != SupplierOrderActionType::Place)
            && replay_capability_ready
            && ensure_replay_safe(&self.db, order, target, executor).await.is_ok()
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
            let resolution = record.verified_resolution()?;
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

    /// 读取实际连接能力并投影结果查询入口，返回重放判断所需的同次连接事实。
    async fn project_query_action(
        &self,
        order: &SupplierFulfillmentOrder,
        target: &SupplierOrderAction,
        allowed_actions: &mut Vec<SupplierOrderAllowedAction>,
        action_blockers: &mut Vec<SupplierOrderActionBlockerView>,
        executor: &mut dyn Executor,
    ) -> Result<(bool, Vec<SupplierApiCapabilityCode>)> {
        let connection =
            self.db.supplier_api_connections().find_by_id(&order.connection_id, executor).await?;
        let connection_active = connection.as_ref().is_some_and(SupplierApiConnection::is_active);
        let capabilities = connection
            .as_ref()
            .and_then(|connection| self.fulfillment.capabilities(connection).ok())
            .unwrap_or_default();

        if order.verified_resolution(target).is_some()
            || ensure_capability(&capabilities, SupplierApiCapabilityCode::Query).is_ok()
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
                    "供应商接口不支持结果查询"
                } else {
                    "供应商接口未配置，且当前业务事实尚不能证明终态"
                },
            ));
        }

        Ok((connection_active, capabilities))
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
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefundFactView>> {
        let bundles = self.db.supplier_fulfillment().refund_fact_bundles_by_order(order_id, executor).await?;
        Ok(bundles.into_iter().map(|bundle| refund_fact_view(&bundle.fact, &bundle.allocations)).collect())
    }
}

/// 正式任务必须精确引用本次订单；不允许其他业务对象的任务放行详情。
fn ensure_task_target(task: &AuthorizedTaskFact, id: &str) -> Result<()> {
    if !matches!(
        task.work_item_type,
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
    ) || task.business_object_type != W26_BUSINESS_OBJECT_TYPE
        || task.business_object_id != id
    {
        return Err(Error::BusinessLogicError("正式任务与当前供应商履约订单不匹配".into()));
    }
    Ok(())
}

fn display_blockers(has_supplier_name: bool) -> Vec<SupplierOrderActionBlockerView> {
    let mut blockers = Vec::new();
    if !has_supplier_name {
        blockers.push(supplier_order_blocker(
            "VIEW_SUPPLIER_NAME",
            "SUPPLIER_NAME_MISSING",
            "供应商主体或当前名称修订缺失，禁止以供应商 ID 伪装名称",
        ));
    }
    blockers.push(supplier_order_blocker(
        "REVEAL_ADDRESS",
        "ADDRESS_REVEAL_NOT_REGISTERED",
        "当前 W26 尚未注册可审计的短时地址揭示入口",
    ));
    blockers
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
        target_supplier_action_id: record.target_supplier_action_id().to_string(),
        outcome: record.outcome(),
        recorded_at: i64::try_from(evidence.base.created_at).unwrap_or(i64::MAX),
        can_safe_retry: record.outcome() == SupplierOrderInvestigationOutcome::VerifiedNoResult,
        external_order_no: order.external_order_no.clone(),
        summary: record.summary().to_string(),
        verified_supplier_action_result_id: (record.outcome()
            == SupplierOrderInvestigationOutcome::VerifiedTerminal)
            .then(|| evidence.base.id.clone()),
        verified_resolution: record.verified_resolution(),
    }
}

use erp_supply::service::supplier_fulfillment::investigate::{
    InvestigationEvidenceRecord, capability_for_action, ensure_replay_safe, parse_investigation_evidence,
    verified_terminal_evidence,
};
use erp_supply::service::supplier_fulfillment::mapping::refund_fact_view;
use erp_supply::service::supplier_fulfillment::place::ensure_capability;
use erp_supply::service::supplier_fulfillment::{
    FulfillmentOrderAccess, SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE,
};

use super::fulfillment_dto::SupplierFulfillmentOrderDetailView;

#[cfg(test)]
mod authorization_tests {
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    struct DeniedTask;

    #[async_trait]
    impl WorkItemAuthorizationReadPort for DeniedTask {
        async fn authorize(
            &self,
            id: &str,
            _: &AuditActor,
            executor: &mut dyn Executor,
        ) -> erp_workflow::Result<(AuthorizedTaskFact, CompiledAccess)> {
            assert_eq!(id, "task");
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            Err(erp_workflow::Error::Forbidden("正式任务无权读取".into()))
        }
    }

    #[tokio::test]
    async fn both_detail_entries_authorize_before_order_and_related_reads() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("fulfillment_detail_authorization");
        let service = SupplierFulfillmentDetailReadService::new(
            db.clone(),
            SupplierFulfillmentService::new(db),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let params = SupplierFulfillmentOrderDetailParams::default();
        let result = service
            .detail_snapshot("order", &params, &actor, &DeniedTask, &mut ReadExecutor::default())
            .await;
        assert!(
            matches!(result, Err(Error::ValidationError(reason)) if reason == "供应商履约详情读取必须使用快照事务")
        );
        let result = service
            .detail_snapshot("order", &params, &actor, &DeniedTask, &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
        let params = SupplierFulfillmentOrderDetailParams { work_item_id: Some(" task ".into()) };
        let result = service
            .detail_snapshot("order", &params, &actor, &DeniedTask, &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(result, Err(Error::Forbidden(reason)) if reason == "正式任务无权读取"));
    }

    #[test]
    fn formal_task_must_reference_the_requested_fulfillment_order() {
        let task = AuthorizedTaskFact {
            work_item_type: WorkItemType::IntegrationResultUnknown,
            business_object_type: W26_BUSINESS_OBJECT_TYPE.into(),
            business_object_id: "order".into(),
            subject_version: "1".into(),
            allowed_actions: vec![],
        };
        ensure_task_target(&task, "order").unwrap();
        assert!(matches!(ensure_task_target(&task, "other"), Err(Error::BusinessLogicError(_))));
        let mut wrong = task.clone();
        wrong.business_object_type = "sales_order".into();
        assert!(matches!(ensure_task_target(&wrong, "order"), Err(Error::BusinessLogicError(_))));
        let mut wrong = task;
        wrong.work_item_type = WorkItemType::DocumentApproval;
        assert!(matches!(ensure_task_target(&wrong, "order"), Err(Error::BusinessLogicError(_))));
    }
}
