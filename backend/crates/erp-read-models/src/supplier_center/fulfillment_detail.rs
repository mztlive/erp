//! 供应商履约详情的跨域只读装配，包括主体名称与 W26 正式任务授权。

use application_core::AuditActor;
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
use erp_supply::repository::prelude::*;
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::work_item::WorkItemAllowedAction;
use persistence_core::NoTransaction;

use crate::ports::work_item_authorization::{AuthorizedTaskFact, WorkItemAuthorizationReadPort};
use crate::purchase_center::repository::supplier_names::current_legal_names_by_account_ids;
use crate::{Error, Result};

/// 组合履约订单、主体名称、调查证据和正式任务授权的详情读取器。
pub struct SupplierFulfillmentDetailReadService {
    pub(super) db: mongodb::Database,
    fulfillment: SupplierFulfillmentService,
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

struct InvestigationAccess {
    can_investigate: bool,
    formal_entry: bool,
}

impl SupplierFulfillmentDetailReadService {
    /// 使用同一数据库和组合根已配置的履约服务创建详情读取器。
    ///
    /// 履约服务负责现有单域订单读取规则；详情读取不调用供应商网关。
    ///
    /// # 参数
    /// * `db` - 数据库。
    /// * `fulfillment` - 组合根已配置的履约读取服务。
    ///
    /// # 返回
    /// 返回详情读取器。构造不执行查询。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database, fulfillment: SupplierFulfillmentService) -> Self {
        Self { db, fulfillment }
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
        task_auth: &dyn WorkItemAuthorizationReadPort,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        let order = self.fulfillment.load_order(id).await?;
        let facts = self.order_detail_facts(&SupplierFulfillmentOrderId::new(id)).await?;
        let supplier_name = current_legal_names_by_account_ids(
            &self.db,
            std::slice::from_ref(&order.supplier_id),
            &mut NoTransaction,
        )
        .await?
        .remove(order.supplier_id.as_ref());
        let mut action_blockers = display_blockers(supplier_name.is_some());
        let projection = self
            .order_action_projection(&order, &facts.actions, params, actor, task_auth, &mut action_blockers)
            .await?;
        let display_items = self.item_display(facts.items).await?;
        let connection_code = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?
            .map(|connection| connection.connection_code);
        Ok(SupplierFulfillmentOrderDetailView {
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
        })
    }

    async fn order_detail_facts(&self, order_id: &SupplierFulfillmentOrderId) -> Result<OrderDetailFacts> {
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(order_id), &mut NoTransaction)
            .await?;
        let actions =
            self.db.supplier_order_actions().list_by_order_newest(order_id, &mut NoTransaction).await?;
        let histories = self
            .db
            .supplier_order_status_histories()
            .list_by_order_chronological(order_id, &mut NoTransaction)
            .await?;
        let refunds = self.refund_views_for_order(order_id).await?;
        Ok(OrderDetailFacts { items, actions, histories, refunds })
    }

    async fn order_action_projection(
        &self,
        order: &SupplierFulfillmentOrder,
        actions: &[SupplierOrderAction],
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        task_auth: &dyn WorkItemAuthorizationReadPort,
        blockers: &mut Vec<SupplierOrderActionBlockerView>,
    ) -> Result<OrderActionProjection> {
        let target = actions.iter().find(|action| action.action_type != SupplierOrderActionType::Query);
        let latest = target.and_then(|target| {
            actions.iter().find_map(|candidate| {
                let record = parse_investigation_evidence(candidate).ok()?;
                (record.target_supplier_action_id() == target.base.id).then_some((candidate, record))
            })
        });
        let access = self.investigation_access(order, params, actor, task_auth, blockers).await?;
        let mut allowed_actions = Vec::new();
        if access.can_investigate {
            if let Some(target) = target {
                self.project_supplier_order_actions(
                    order,
                    target,
                    latest.as_ref(),
                    access.formal_entry,
                    &mut allowed_actions,
                    blockers,
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
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        task_auth: &dyn WorkItemAuthorizationReadPort,
        blockers: &mut Vec<SupplierOrderActionBlockerView>,
    ) -> Result<InvestigationAccess> {
        let work_item_id = params.work_item_id.as_deref().map(str::trim).filter(|value| !value.is_empty());
        let formal = self.formal_task(order, work_item_id, actor, task_auth).await?;
        let has_active_task = self
            .db
            .work_items()
            .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, &order.base.id, &mut NoTransaction)
            .await?
            .into_iter()
            .next()
            .is_some();
        let can_investigate = if let Some(formal) = &formal {
            if !formal.allowed_actions.contains(&WorkItemAllowedAction::Process) {
                block_supplier_order_domain_actions(
                    blockers,
                    "CURRENT_RESPONSIBILITY_REQUIRED",
                    "当前账号不是开放任务的当前责任人",
                );
                false
            } else {
                self.formal_actor_eligible(work_item_id.expect("formal work item id"), actor, blockers)
                    .await?
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

    async fn formal_task(
        &self,
        order: &SupplierFulfillmentOrder,
        work_item_id: Option<&str>,
        actor: &AuditActor,
        task_auth: &dyn WorkItemAuthorizationReadPort,
    ) -> Result<Option<AuthorizedTaskFact>> {
        let Some(work_item_id) = work_item_id else {
            return Ok(None);
        };
        let view = task_auth.authorize(work_item_id, actor).await?;
        if !matches!(
            view.work_item_type,
            WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
        ) || view.business_object_type != W26_BUSINESS_OBJECT_TYPE
            || view.business_object_id != order.base.id
            || view.subject_version != order.base.version.to_string()
        {
            return Err(Error::BusinessLogicError("正式任务与当前供应商履约订单不匹配".to_string()));
        }
        Ok(Some(view))
    }

    async fn formal_actor_eligible(
        &self,
        work_item_id: &str,
        actor: &AuditActor,
        blockers: &mut Vec<SupplierOrderActionBlockerView>,
    ) -> Result<bool> {
        let raw = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
        if ensure_task_actor_eligible(&self.db, &raw, actor.id(), &mut NoTransaction).await.is_err() {
            block_supplier_order_domain_actions(
                blockers,
                "ACTOR_INELIGIBLE",
                "当前账号已不具备该供应商履约任务的角色或组织资格",
            );
            return Ok(false);
        }
        Ok(true)
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
        let connection =
            self.db.supplier_api_connections().find_by_id(&order.connection_id, &mut NoTransaction).await?;
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
            && ensure_replay_safe(&self.db, order, target, &mut NoTransaction).await.is_ok()
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
        let bundles =
            self.db.supplier_fulfillment().refund_fact_bundles_by_order(order_id, &mut NoTransaction).await?;
        Ok(bundles.into_iter().map(|bundle| refund_fact_view(&bundle.fact, &bundle.allocations)).collect())
    }
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
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};

use super::fulfillment_access::ensure_task_actor_eligible;
use super::fulfillment_dto::SupplierFulfillmentOrderDetailView;
