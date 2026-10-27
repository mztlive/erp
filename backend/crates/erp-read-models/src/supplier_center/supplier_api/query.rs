use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{PartyId, SupplierAccountId, SupplierApiConnectionId};
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use erp_supplier::repository::prelude::*;
use erp_supply::dto::supplier_api::{
    PageView, SafeReferencesView, SupplierActionBlockerView, SupplierApiCapabilitySummaryView,
    SupplierApiConnectionDetailView, SupplierApiConnectionListItemView, SupplierApiConnectionListParams,
    SupplierApiConnectionView, SupplierHealthCheckRunView,
};
use erp_supply::entity::supplier_api::{
    SupplierApiCapabilityCode, SupplierApiCapabilityStatus, SupplierApiConnection, SupplierConnectionAction,
    SupplierConnectionGovernance, SupplierHealthCheckRun, SupplierHealthCheckType,
};
use erp_supply::ports::supplier_reference_registry::SupplierReferenceTarget;
use erp_supply::service::supplier_api::context::{blocker, governance_blocker_view, safe_reference};
use erp_support::BulkJobExt;
use erp_support::repository::prelude::*;
use persistence_core::NoTransaction;

use super::SupplierApiReadService;
use super::context::GovernanceContext;
use crate::Result;

impl SupplierApiReadService {
    /// 按查询参数与服务端连接事实返回分页投影。
    ///
    /// # 参数
    /// * `params` - 连接列表查询参数。
    /// * `_actor` - 保留的操作人。当前不参与过滤或动作投影。
    ///
    /// # 返回
    /// 返回带能力摘要和供应商名称的连接分页。空关键词不按名称收窄。
    ///
    /// # 错误
    /// 参数校验失败时返回 `ValidationError`。供应商名称、连接或能力读取失败时返回对应错误。
    pub async fn connection_list_for_actor(
        &self,
        params: &SupplierApiConnectionListParams,
        _actor: &AuditActor,
    ) -> Result<PageView<SupplierApiConnectionListItemView>> {
        validator::Validate::validate(params)?;
        let suppliers = crate::supplier_center::keyword_supplier_ids(&self.db, params.q.as_deref()).await?;
        let page = self.domain().connection_list(params, suppliers).await?;
        let supplier_names = self.supplier_names_for_connections(&page.items).await?;
        let items = page
            .items
            .into_iter()
            .map(|connection| SupplierApiConnectionListItemView {
                supplier_name: supplier_names.get(&connection.supplier_id).cloned(),
                capabilities: self
                    .reference_registry
                    .as_ref()
                    .and_then(|registry| {
                        registry
                            .capabilities(&SupplierReferenceTarget {
                                connection_id: SupplierApiConnectionId::new(&connection.id),
                                supplier_id: SupplierAccountId::new(&connection.supplier_id),
                                connector_type: connection.connector_type,
                            })
                            .ok()
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .map(|capability_code| SupplierApiCapabilitySummaryView {
                        capability_code,
                        status: SupplierApiCapabilityStatus::Active,
                    })
                    .collect(),
                connection,
            })
            .collect();
        Ok(PageView { items, total: page.total, page: page.page, page_size: page.page_size })
    }

    async fn supplier_names_for_connections(
        &self,
        connections: &[SupplierApiConnectionView],
    ) -> Result<HashMap<String, String>> {
        let supplier_ids =
            connections.iter().map(|item| SupplierAccountId::new(&item.supplier_id)).collect::<Vec<_>>();
        let accounts =
            self.db.supplier_accounts().find_accounts_by_ids(&supplier_ids, &mut NoTransaction).await?;
        let party_ids =
            accounts.iter().map(|account| PartyId::new(account.party_id.to_string())).collect::<Vec<_>>();
        let (parties, revisions) =
            self.db.party().list_with_current_revisions(&party_ids, &mut NoTransaction).await?;
        let revisions_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision.legal_name))
            .collect::<HashMap<_, _>>();
        let names_by_party = parties
            .into_iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id?;
                let name = revisions_by_id.get(&revision_id)?.clone();
                Some((party.base.id, name))
            })
            .collect::<HashMap<_, _>>();
        Ok(accounts
            .into_iter()
            .filter_map(|account| {
                let name = names_by_party.get(account.party_id.as_ref())?.clone();
                Some((account.base.id, name))
            })
            .collect())
    }

    /// 返回服务端权威动作、阻塞原因和安全引用投影的连接详情。
    ///
    /// # 参数
    /// * `id` - 连接主键。
    /// * `actor` - 已认证操作人，用于动作和引用元数据权限。
    ///
    /// # 返回
    /// 返回连接详情投影。
    ///
    /// # 错误
    /// 连接不存在时返回 `NotFound`。查询或 RBAC 判定失败时返回对应错误。
    pub async fn connection_detail_for_actor(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionDetailView> {
        let connection = self.domain().load_connection(id, &mut NoTransaction).await?;
        let context = self.governance_context(&connection, &mut NoTransaction).await?;
        self.detail_view(connection, context, actor).await
    }

    async fn detail_view(
        &self,
        connection: SupplierApiConnection,
        context: GovernanceContext,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionDetailView> {
        let reference_visible =
            self.has_permission(actor, "supplier_api_connection:view_reference_metadata").await?;
        let declared_capabilities = self
            .reference_registry
            .as_ref()
            .and_then(|registry| registry.capabilities(&SupplierReferenceTarget::from(&connection)).ok())
            .unwrap_or_default();

        let (allowed_actions, action_blockers) =
            self.connection_action_projection(&connection, &context, actor).await?;
        let mut connection_view: SupplierApiConnectionView = connection.clone().into();
        connection_view.safe_references = SafeReferencesView {
            endpoint: safe_reference(connection.endpoint_reference_bound, reference_visible),
            credential: safe_reference(connection.credential_reference_bound, reference_visible),
        };
        connection_view.allowed_actions = allowed_actions;
        connection_view.action_blockers = action_blockers;
        let deployment_ready = self.reference_registry.as_ref().is_some_and(|registry| {
            registry.deployment_config(&SupplierReferenceTarget::from(&connection)).is_ok()
        });
        Ok(SupplierApiConnectionDetailView {
            deployment_ready,
            deployment_message: if deployment_ready {
                "已自动使用服务端接口地址和凭据"
            } else {
                "服务端接口配置未就绪，请联系管理员配置并重启服务"
            }
            .into(),
            declared_capabilities,
            connection: connection_view,
            health_records: context.health_runs.iter().map(health_run_view).collect(),
            health_check_types: vec![
                SupplierHealthCheckType::Connectivity,
                SupplierHealthCheckType::Authentication,
            ],
        })
    }

    async fn connection_action_projection(
        &self,
        connection: &SupplierApiConnection,
        _context: &GovernanceContext,
        actor: &AuditActor,
    ) -> Result<(Vec<String>, Vec<SupplierActionBlockerView>)> {
        let mut allowed = Vec::new();
        let mut blocked = Vec::new();
        let active_catalog =
            self.db.background_jobs().active_catalog_jobs(&connection.base.id, &mut NoTransaction).await?;
        let catalog_running = active_catalog.len() >= 100
            || active_catalog.iter().any(|job| !job.catalog_stalled(Instant::now()));
        let deployment = self
            .reference_registry
            .as_ref()
            .and_then(|registry| registry.deployment_config(&SupplierReferenceTarget::from(connection)).ok());
        for action in SupplierConnectionAction::all() {
            if matches!(
                action,
                SupplierConnectionAction::BindEndpointReference
                    | SupplierConnectionAction::BindCredentialReference
            ) {
                continue;
            }
            if !self.has_action_permission(actor, action).await? {
                continue;
            }
            let blockers = self.operation_blockers(connection, action, deployment.is_some(), catalog_running);
            if blockers.is_empty() {
                allowed.push(action.as_str().to_string());
            } else {
                blocked.extend(blockers);
            }
        }
        Ok((allowed, blocked))
    }

    /// 只读动作资格；开启时的真实鉴权由命令自动执行。
    fn operation_blockers(
        &self,
        connection: &SupplierApiConnection,
        action: SupplierConnectionAction,
        configured: bool,
        catalog_running: bool,
    ) -> Vec<SupplierActionBlockerView> {
        if let Some(blocker) = operation_blocker(action, catalog_running) {
            return vec![blocker];
        }
        if action == SupplierConnectionAction::Enable && !connection.is_active() {
            let supported = self
                .reference_registry
                .as_ref()
                .and_then(|registry| registry.capabilities(&SupplierReferenceTarget::from(connection)).ok())
                .is_some_and(|codes| codes.contains(&SupplierApiCapabilityCode::Order));
            return if configured && supported {
                Vec::new()
            } else {
                vec![blocker(action.as_str(), "ORDER_UNAVAILABLE", "接口尚未配置或不支持自动下单", None)]
            };
        }
        SupplierConnectionGovernance { connection, capabilities: &[], confirmations: &[], health_runs: &[] }
            .blockers(action, Default::default(), configured)
            .into_iter()
            .map(governance_blocker_view)
            .collect()
    }
}

fn health_run_view(run: &SupplierHealthCheckRun) -> SupplierHealthCheckRunView {
    SupplierHealthCheckRunView {
        id: run.base.id.clone(),
        job_id: run.background_job_id.clone(),
        check_type: run.check_type,
        status: run.status,
        technical_config_version: run.technical_config_version,
        requested_by: run.requested_by.clone(),
        started_at: run.started_at.map(|at| at.unix_secs() as u64),
        finished_at: run.finished_at.map(|at| at.unix_secs() as u64),
        latency_ms: run.latency_ms,
        error_code: run.error_code.clone(),
        error_summary: run.error_summary.clone(),
    }
}

/// 将运行中任务和部署变化转换为可直接操作的提示。
fn operation_blocker(action: SupplierConnectionAction, running: bool) -> Option<SupplierActionBlockerView> {
    match action {
        SupplierConnectionAction::StartCatalogSync if running => {
            Some(blocker(action.as_str(), "CATALOG_SYNC_RUNNING", "商品同步正在运行，请等待完成", None))
        },
        _ => None,
    }
}
