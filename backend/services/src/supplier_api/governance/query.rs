use std::collections::HashMap;

use database::SupplierApiExt;
use entities::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionStatus, SupplierConnectionAction,
    SupplierConnectionGovernance, SupplierHealthCheckRun, SupplierHealthCheckType,
};
use erp_core::ids::{PartyId, SupplierAccountId, SupplierApiConnectionId};
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use persistence_core::NoTransaction;

use crate::errors::Result;
use application_core::AuditActor;

use super::super::dto::{
    SafeReferencesView, SupplierActionBlockerView, SupplierApiCapabilitySummaryView,
    SupplierApiCapabilityView, SupplierApiConnectionDetailView, SupplierApiConnectionListItemView,
    SupplierApiConnectionListParams, SupplierApiConnectionView, SupplierHealthCheckRunView,
};
use super::super::{PageView, SupplierApiService};
use super::context::{blocker, governance_blocker_view, impact_view, safe_reference, GovernanceContext};

const CONFIRM_CAPABILITY_ACTION: &str = "CONFIRM_BUSINESS_CAPABILITY_REQUIREMENT";
const UPDATE_CAPABILITIES_ACTION: &str = "UPDATE_CAPABILITIES";

impl SupplierApiService {
    /// 按当前操作人的权限与服务端业务事实返回连接分页投影。
    ///
    /// # Errors
    /// 查询、授权源或动作投影失败时返回错误。
    pub async fn connection_list_for_actor(
        &self,
        params: &SupplierApiConnectionListParams,
        _actor: &AuditActor,
    ) -> Result<PageView<SupplierApiConnectionListItemView>> {
        let page = self.connection_list(params).await?;
        let connection_ids = page
            .items
            .iter()
            .map(|item| SupplierApiConnectionId::new(&item.id))
            .collect::<Vec<_>>();
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connections(&connection_ids, &mut NoTransaction)
            .await?;
        let capabilities_by_connection = capabilities.into_iter().fold(
            HashMap::<String, Vec<SupplierApiCapabilitySummaryView>>::new(),
            |mut grouped, capability| {
                grouped
                    .entry(capability.connection_id.to_string())
                    .or_default()
                    .push(SupplierApiCapabilitySummaryView {
                        capability_code: capability.capability_code,
                        status: capability.status,
                    });
                grouped
            },
        );
        let supplier_names = self.supplier_names_for_connections(&page.items).await?;
        let items = page
            .items
            .into_iter()
            .map(|connection| SupplierApiConnectionListItemView {
                supplier_name: supplier_names.get(&connection.supplier_id).cloned(),
                capabilities: capabilities_by_connection
                    .get(&connection.id)
                    .cloned()
                    .unwrap_or_default(),
                connection,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }

    async fn supplier_names_for_connections(
        &self,
        connections: &[SupplierApiConnectionView],
    ) -> Result<HashMap<String, String>> {
        let supplier_ids = connections
            .iter()
            .map(|item| SupplierAccountId::new(&item.supplier_id))
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .supplier_accounts()
            .find_accounts_by_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let party_ids = accounts
            .iter()
            .map(|account| PartyId::new(account.party_id.to_string()))
            .collect::<Vec<_>>();
        let (parties, revisions) = self
            .db
            .party()
            .list_with_current_revisions(&party_ids, &mut NoTransaction)
            .await?;
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
    /// # Errors
    /// 连接不存在、查询失败或 RBAC 无法取得稳定快照时返回错误。
    pub async fn connection_detail_for_actor(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionDetailView> {
        let connection = self.load_connection(id, &mut NoTransaction).await?;
        let context = self.governance_context(&connection, &mut NoTransaction).await?;
        self.detail_view(connection, context, actor).await
    }

    async fn detail_view(
        &self,
        connection: SupplierApiConnection,
        context: GovernanceContext,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionDetailView> {
        let reference_visible = self
            .has_permission(actor, "supplier_api_connection:view_reference_metadata")
            .await?;
        let governance = SupplierConnectionGovernance {
            connection: &connection,
            capabilities: &context.capabilities,
            confirmations: &context.confirmations,
            health_runs: &context.health_runs,
        };
        let latest_success = governance.latest_successful_health_run();
        let can_confirm = self
            .has_permission(actor, "supplier_api_capability:confirm_requirement")
            .await?;
        let can_update_capability = self
            .has_permission(actor, "supplier_api_capability:update")
            .await?;
        let mut capabilities = Vec::with_capacity(context.capabilities.len());
        for capability in &context.capabilities {
            let confirmation = governance.latest_confirmation(capability.capability_code);
            let verified = latest_success.is_some_and(|run| run.verifies(capability));
            let mut allowed_actions = Vec::new();
            let mut action_blockers = Vec::new();
            if can_confirm {
                allowed_actions.push(CONFIRM_CAPABILITY_ACTION.to_string());
            }
            if can_update_capability {
                if connection.stable.status == SupplierApiConnectionStatus::Active {
                    action_blockers.push(blocker(
                        UPDATE_CAPABILITIES_ACTION,
                        "CONNECTION_ENABLED",
                        "请先停用连接，再修改能力配置",
                        None,
                    ));
                } else {
                    allowed_actions.push(UPDATE_CAPABILITIES_ACTION.to_string());
                }
            }
            capabilities.push(SupplierApiCapabilityView {
                id: capability.base.id.clone(),
                connection_id: connection.base.id.clone(),
                capability_code: capability.capability_code,
                status: capability.status,
                version: capability.base.version,
                created_at: capability.base.created_at,
                constraint_summary: capability.constraint_snapshot.clone(),
                business_requirement: confirmation.map(|value| value.requirement),
                business_confirmation_version: confirmation.map(|value| value.base.version),
                technically_verified: verified,
                verified_at: latest_success
                    .filter(|_| verified)
                    .and_then(|run| run.finished_at)
                    .map(|at| at.unix_secs() as u64),
                allowed_actions,
                action_blockers,
            });
        }

        let (allowed_actions, action_blockers) = self
            .connection_action_projection(&connection, &context, actor)
            .await?;
        let mut connection_view: SupplierApiConnectionView = connection.clone().into();
        connection_view.safe_references = SafeReferencesView {
            endpoint: safe_reference(connection.endpoint_reference_bound, reference_visible),
            credential: safe_reference(connection.credential_reference_bound, reference_visible),
        };
        connection_view.allowed_actions = allowed_actions;
        connection_view.action_blockers = action_blockers;
        Ok(SupplierApiConnectionDetailView {
            connection: connection_view,
            capabilities,
            health_records: context.health_runs.iter().map(health_run_view).collect(),
            health_check_types: vec![
                SupplierHealthCheckType::Connectivity,
                SupplierHealthCheckType::Authentication,
                SupplierHealthCheckType::CapabilityMetadata,
            ],
            related_impact: impact_view(context.impact),
        })
    }

    async fn connection_action_projection(
        &self,
        connection: &SupplierApiConnection,
        context: &GovernanceContext,
        actor: &AuditActor,
    ) -> Result<(Vec<String>, Vec<SupplierActionBlockerView>)> {
        let mut allowed = Vec::new();
        let mut blocked = Vec::new();
        let governance = SupplierConnectionGovernance {
            connection,
            capabilities: &context.capabilities,
            confirmations: &context.confirmations,
            health_runs: &context.health_runs,
        };
        for action in SupplierConnectionAction::all() {
            if !self.has_action_permission(actor, action).await? {
                continue;
            }
            let blockers =
                governance.blockers(action, context.impact, self.reference_registry.is_available());
            if blockers.is_empty() {
                allowed.push(action.as_str().to_string());
            } else {
                blocked.extend(blockers.into_iter().map(governance_blocker_view));
            }
        }
        Ok((allowed, blocked))
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
