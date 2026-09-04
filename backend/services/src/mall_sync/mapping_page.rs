//! 映射任务列表页批量装载（INT-R17）。
//!
//! 列表页按当前页任务 ID 固定次数批量装载完整任务、来源快照、正式责任行、
//! 来源系统、最新归集操作、审计时间线与谱系映射/目标，查询次数不随页内行数
//! 增长；候选目标按映射类型每页至多计算一次。缺失、损坏与稳定排序语义与逐行
//! 旧路径一致，RBAC、候选项、allowed actions 与响应映射仍由 Service 解释。

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use database::{AccessControlExt, MallSyncExt, NoTransaction, SourceRegistryExt, WorkItemExt};
use entities::mall_sync::{
    MallSalesOrderSnapshot, MappingSourceIdentity, MappingTaskStatus, MappingTaskType, MasterMappingTask,
};
use entities::source_registry::{
    ExternalIdentityMap, ExternalIdentityTarget, MallSyncStage, SourceSystem, SourceSystemStatus,
    SourceSystemType,
};
use entities::work_item::{WorkItem, WorkItemStatus};
use entities::AuditLog;

use super::dto::{
    MappingActionBlockerView, MappingCandidateTargetView, MappingCurrentTargetView,
    MappingResolutionHistoryView, MappingSourceEvidenceView, MappingTaskWorkItemView,
    MasterMappingTaskListParams, MasterMappingTaskView, OwnerRoutingState, PageView, SortDir,
};
use super::{snapshot_identity_error, MallSyncService, MasterMappingTaskFilter};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use validator::Validate;

/// 按键索引实体（后出现的覆盖先出现的；调用方保证键唯一）。
///
/// # 参数
/// * `items` - 待索引实体
/// * `key` - 键提取器
///
/// # 返回
/// 返回按键索引的映射。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存函数；重复键保留最后一条，调用方不得依赖重复输入。
fn index_by_key<T, K>(items: Vec<T>, key: impl Fn(&T) -> K) -> HashMap<K, T>
where
    K: Eq + Hash,
{
    let mut map = HashMap::with_capacity(items.len());
    for item in items {
        map.insert(key(&item), item);
    }
    map
}

/// 按键归组实体（组内保持批量查询的稳定顺序）。
///
/// # 参数
/// * `items` - 待归组实体（已按稳定顺序排列）
/// * `key` - 键提取器
///
/// # 返回
/// 返回按键归组的映射；缺项表示该键无事实。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存函数；组内顺序与输入一致，调用方必须传入稳定排序的批量结果。
fn group_by_key<T, K>(items: Vec<T>, key: impl Fn(&T) -> K) -> HashMap<K, Vec<T>>
where
    K: Eq + Hash,
{
    let mut map: HashMap<K, Vec<T>> = HashMap::new();
    for item in items {
        map.entry(key(&item)).or_default().push(item);
    }
    map
}

/// 将审计记录映射为解决历史视图（INT-R17 纯映射）。
///
/// 与单任务时间线同序（创建时间与 ID 升序由批量查询保证）。
///
/// # 参数
/// * `audits` - 已按稳定顺序排列的审计记录
///
/// # 返回
/// 返回解决历史视图。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯映射函数；调用方必须传入稳定排序的审计记录。
pub(super) fn history_views(audits: &[AuditLog]) -> Vec<MappingResolutionHistoryView> {
    audits
        .iter()
        .map(|audit| MappingResolutionHistoryView {
            action: audit.action.clone(),
            result: if audit.success { "SUCCEEDED" } else { "FAILED" }.to_string(),
            handled_by: audit.actor_id.clone(),
            handled_at: audit.base.created_at,
            evidence_reference: Some(audit.base.id.clone()),
        })
        .collect()
}

/// 将谱系目标映射为当前目标视图（INT-R17 纯映射）。
///
/// 与单任务谱系查询同序（`valid_from` 降序、ID 升序由批量查询保证）。
///
/// # 参数
/// * `targets` - 已按稳定顺序排列的谱系目标
///
/// # 返回
/// 返回当前目标视图。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯映射函数；调用方必须传入稳定排序的谱系目标。
fn current_target_views(targets: &[ExternalIdentityTarget]) -> Vec<MappingCurrentTargetView> {
    targets
        .iter()
        .map(|target| MappingCurrentTargetView {
            mapping_target_id: target.base.id.clone(),
            object_type: target.internal_object_type.as_str().to_string(),
            object_id: target.internal_object_id.clone(),
            relation_role: target.relation_role,
            valid_from: target.valid_from,
            valid_to: target.valid_to,
            status: target.status.as_str().to_string(),
        })
        .collect()
}

/// 构造映射阻断项（与详情路径同一 truth table 输入）。
///
/// # 参数
/// * `action` - 被阻断的动作
/// * `code` - 阻断代码
/// * `message` - 阻断说明
///
/// # 返回
/// 返回动作阻断视图。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯构造，阻断语义由装配 truth table 决定。
fn mapping_blocker(action: &str, code: &str, message: &str) -> MappingActionBlockerView {
    MappingActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// 从快照提取来源证据（与详情路径同一证据集）。
///
/// # 参数
/// * `snapshot` - 规范化商城快照
/// * `mapping_type` - 当前映射差异类型
///
/// # 返回
/// 返回来源证据视图。
///
/// # 错误
/// 无错误返回：快照解析失败时只返回基础证据。
///
/// # 约束
/// 纯只读投影，不访问数据库；证据集与旧详情路径一致。
fn mapping_source_evidence(
    snapshot: &MallSalesOrderSnapshot,
    mapping_type: MappingTaskType,
) -> Vec<MappingSourceEvidenceView> {
    let mut evidence = vec![
        MappingSourceEvidenceView {
            field: "external_order_no".to_string(),
            label: "商城销售单号".to_string(),
            value: snapshot.external_order_no.clone(),
            sensitive: false,
        },
        MappingSourceEvidenceView {
            field: "source_status_code".to_string(),
            label: "来源状态码".to_string(),
            value: snapshot.source_status_code.clone(),
            sensitive: false,
        },
        MappingSourceEvidenceView {
            field: "source_updated_at".to_string(),
            label: "来源更新时间".to_string(),
            value: snapshot.source_updated_at.unix_secs().to_string(),
            sensitive: false,
        },
    ];
    let Some(registration) = mapping_type.target_registration() else {
        return evidence;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&snapshot.normalized_snapshot) else {
        return evidence;
    };
    let Some(object) = value.as_object() else {
        return evidence;
    };
    for field in registration.source_identity_fields {
        let Some(value) = object.get(*field) else {
            continue;
        };
        let Ok(value) = MappingSourceIdentity::from_json_value(value) else {
            continue;
        };
        evidence.push(MappingSourceEvidenceView {
            field: (*field).to_string(),
            label: MappingTaskType::source_identity_label(field).to_string(),
            value: value.into_string(),
            sensitive: false,
        });
    }
    evidence
}

/// 映射任务视图装配所需的预加载事实（列表批量与详情单读共用）。
///
/// # 约束
/// 只承载已授权解释前的持久化事实与只读投影，不含事务与网关。
pub(super) struct MappingTaskFacts {
    /// 来源快照（缺失由调用方失败关闭）。
    pub snapshot: MallSalesOrderSnapshot,
    /// 唯一正式责任任务；`None` 表示尚无责任。
    pub work_item: Option<WorkItem>,
    /// 来源商城（缺失由调用方失败关闭）。
    pub source: SourceSystem,
    /// 候选目标（RBAC 与阶段门控后）。
    pub candidate_targets: Vec<MappingCandidateTargetView>,
    /// 外部身份谱系 ID；无谱系时为 `None`。
    pub external_identity_map_id: Option<String>,
    /// 当前谱系目标视图。
    pub current_targets: Vec<MappingCurrentTargetView>,
    /// 来源身份解析失败时的错误文本。
    pub lineage_error: Option<String>,
    /// 最新归集操作视图；无操作时为 `None`。
    pub reapply_operation: Option<super::dto::ReapplyOperationView>,
    /// 解决历史视图（稳定顺序）。
    pub resolution_history: Vec<MappingResolutionHistoryView>,
}

/// 组装映射任务视图（列表页与详情共用的唯一规则源）。
///
/// # 参数
/// * `task` - 映射任务
/// * `facts` - 预加载的支撑事实
/// * `actor` - 当前认证操作人
///
/// # 返回
/// 返回契约形状的任务视图。
///
/// # 错误
/// 无错误返回：缺失与损坏由事实装载阶段失败关闭。
///
/// # 约束
/// 纯装配函数，不访问数据库；RBAC、候选项与动作 truth table 与旧详情一致。
pub(super) fn assemble_mapping_task_view(
    task: MasterMappingTask,
    facts: MappingTaskFacts,
    actor: &AuditActor,
) -> MasterMappingTaskView {
    let MappingTaskFacts {
        snapshot,
        work_item,
        source,
        candidate_targets,
        external_identity_map_id,
        current_targets,
        lineage_error,
        reapply_operation,
        resolution_history,
    } = facts;
    let routing_configured = task.owner_role.is_some() && work_item.is_some();
    let eligible = work_item.is_some();
    let stage_active = source.system_type == SourceSystemType::Mall
        && source.stable.status == SourceSystemStatus::Active
        && source.mall_sync_stage == Some(MallSyncStage::FirstPhaseMallOwned);
    let source_evidence = mapping_source_evidence(&snapshot, task.mapping_type);

    let mut allowed_actions = Vec::new();
    let mut action_blockers = Vec::new();
    let owns_open_task = work_item
        .as_ref()
        .is_some_and(|item| item.status == WorkItemStatus::Open && item.is_owned_by(actor.id()) && eligible);
    if !stage_active {
        for action in ["CONFIRM_TARGET", "REQUEST_SOURCE_FIX", "REAPPLY"] {
            action_blockers.push(mapping_blocker(
                action,
                "MALL_SYNC_ARCHIVED",
                "来源商城未处于一期可写阶段，W17 仅保留历史查询",
            ));
        }
    } else {
        match task.status {
            MappingTaskStatus::Pending => {
                if !routing_configured {
                    action_blockers.push(mapping_blocker(
                        "CONFIRM_TARGET",
                        "OWNER_ROUTING_MISSING",
                        "当前映射类型尚未形成唯一责任路由与正式任务",
                    ));
                    action_blockers.push(mapping_blocker(
                        "REQUEST_SOURCE_FIX",
                        "OWNER_ROUTING_MISSING",
                        "当前映射类型尚未形成唯一责任路由与正式任务",
                    ));
                } else if owns_open_task {
                    allowed_actions.push("REQUEST_SOURCE_FIX".to_string());
                    if task.mapping_type.target_registration().is_none() {
                        action_blockers.push(mapping_blocker(
                            "CONFIRM_TARGET",
                            "MAPPING_TYPE_NOT_REGISTERED",
                            "该差异类型没有独立 ERP 规范目标模型，只能追加来源修复证据",
                        ));
                    } else if candidate_targets.is_empty() {
                        action_blockers.push(mapping_blocker(
                            "CONFIRM_TARGET",
                            "TARGET_CANDIDATE_EMPTY",
                            "当前责任与数据范围内没有可确认的有效 ERP 目标",
                        ));
                    } else if let Some(message) = lineage_error.as_deref() {
                        action_blockers.push(mapping_blocker(
                            "CONFIRM_TARGET",
                            "SOURCE_IDENTITY_INVALID",
                            message,
                        ));
                    } else {
                        allowed_actions.push("CONFIRM_TARGET".to_string());
                    }
                } else {
                    action_blockers.push(mapping_blocker(
                        "CONFIRM_TARGET",
                        "RESPONSIBILITY_NOT_HELD",
                        "当前账号尚未取得该正式任务的个人责任",
                    ));
                    action_blockers.push(mapping_blocker(
                        "REQUEST_SOURCE_FIX",
                        "RESPONSIBILITY_NOT_HELD",
                        "当前账号尚未取得该正式任务的个人责任",
                    ));
                }
            }
            MappingTaskStatus::Resolved
                if eligible && routing_configured && snapshot.applied_sales_order_revision_id.is_some() =>
            {
                allowed_actions.push("REAPPLY".to_string());
            }
            MappingTaskStatus::Resolved if eligible && routing_configured => {
                action_blockers.push(mapping_blocker(
                    "REAPPLY",
                    "REAPPLY_EXECUTOR_UNAVAILABLE",
                    "当前环境尚未注册原快照归集执行器，禁止把排队或固定失败视为归集成功",
                ));
            }
            MappingTaskStatus::Resolved => action_blockers.push(mapping_blocker(
                "REAPPLY",
                "RESPONSIBILITY_NOT_ELIGIBLE",
                "当前账号不具备该映射责任角色资格",
            )),
            MappingTaskStatus::Unresolvable | MappingTaskStatus::Closed => {
                action_blockers.push(mapping_blocker(
                    "REAPPLY",
                    "MAPPING_TASK_NOT_RESOLVED",
                    "只有已解决的映射任务可以重新归集",
                ));
            }
        }
    }

    let projected_work_item = work_item.as_ref().map(|item| MappingTaskWorkItemView {
        work_item_id: item.base.id.clone(),
        task_version: item.base.version.to_string(),
        work_item_type: item.work_item_type,
        business_object_type: item.business_object_type.clone(),
        business_object_id: item.business_object_id.clone(),
        subject_version: item.subject_version.clone(),
        status: item.status,
        owner_user_id: item.owner_user_id.clone(),
    });
    let owner_role = routing_configured
        .then(|| work_item.as_ref().map(|item| item.owner_role.clone()))
        .flatten();
    let owner_user_id = routing_configured
        .then(|| work_item.as_ref().and_then(|item| item.owner_user_id.clone()))
        .flatten();
    let lock_version = task.base.version;
    MasterMappingTaskView {
        id: task.base.id,
        source_snapshot_id: task.source_snapshot_id.to_string(),
        mapping_type: task.mapping_type,
        status: task.status,
        owner_role,
        owner_user_id,
        resolution: task.resolution,
        resolved_at: task.resolved_at,
        version: lock_version,
        created_at: task.base.created_at,
        owner_routing_state: if routing_configured {
            OwnerRoutingState::Configured
        } else {
            OwnerRoutingState::Missing
        },
        work_item: routing_configured.then_some(projected_work_item).flatten(),
        source_evidence,
        candidate_targets,
        current_targets,
        external_identity_map_id,
        impact_summary: format!(
            "{}映射未完成将阻断正确客户、应收、收入或经营归属，来源捕获水位不回退",
            task.mapping_type.label()
        ),
        resolution_history,
        allowed_actions,
        action_blockers,
        reapply_operation,
        lock_version,
    }
}

impl MallSyncService {
    /// 分页查询映射任务列表（INT-R17 批量装载）。
    ///
    /// 先分页取投影行，再按页内任务 ID 固定次数批量装载完整任务、来源快照、
    /// 正式责任行、来源系统、最新归集操作、审计时间线与谱系映射/目标；候选
    /// 目标按映射类型每页至多计算一次。页内顺序、基数、缺失与损坏语义与逐行
    /// 旧路径一致。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 当前认证操作人
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    /// * `Internal` - 投影行缺失完整任务、快照、来源或责任事实不唯一
    ///
    /// # 约束
    /// 不改变软删除、当前指针、业务日期、稳定排序与首错语义。
    pub async fn mapping_task_list(
        &self,
        params: &MasterMappingTaskListParams,
        actor: &AuditActor,
    ) -> Result<PageView<MasterMappingTaskView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MasterMappingTaskFilter {
            source_snapshot_id: query.source_snapshot_id,
            mapping_type: query.mapping_type,
            status: query.status,
            owner_role: query.owner_role,
            owner_user_id: query.owner_user_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .master_mapping_tasks()
            .search_master_mapping_tasks(&filter, &mut NoTransaction)
            .await?;
        let task_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let tasks = self
            .db
            .master_mapping_tasks()
            .find_mapping_tasks_by_ids(&task_ids, &mut NoTransaction)
            .await?;
        let task_map = index_by_key(tasks, |task| task.base.id.clone());
        let mut ordered_tasks = Vec::with_capacity(page.items.len());
        for row in &page.items {
            let task = task_map
                .get(&row.id)
                .cloned()
                .ok_or_else(|| Error::Internal("映射任务列表投影对应的领域对象不存在".to_string()))?;
            ordered_tasks.push(task);
        }
        let snapshot_ids = ordered_tasks
            .iter()
            .map(|task| task.source_snapshot_id.to_string())
            .collect::<Vec<_>>();
        let snapshots = self
            .db
            .mall_sales_order_snapshots()
            .find_snapshots_by_ids(&snapshot_ids, &mut NoTransaction)
            .await?;
        let snapshot_map = index_by_key(snapshots, |snapshot| snapshot.base.id.clone());
        let work_items = self
            .db
            .work_items()
            .list_for_master_mapping_tasks(&task_ids, &mut NoTransaction)
            .await?;
        let work_groups = group_by_key(work_items, |item| item.business_object_id.clone());
        let mut seen_sources = HashSet::new();
        let mut source_ids = Vec::new();
        for snapshot in snapshot_map.values() {
            let key = snapshot.source_system_id.to_string();
            if seen_sources.insert(key) {
                source_ids.push(snapshot.source_system_id.clone());
            }
        }
        let sources = self
            .db
            .source_systems()
            .find_systems_by_ids(&source_ids, &mut NoTransaction)
            .await?;
        let source_map = index_by_key(sources, |source| source.base.id.clone());
        let reapply_map = self
            .db
            .mall_snapshot_reapply_operations()
            .find_reapply_latest_by_task_ids(&task_ids, &mut NoTransaction)
            .await?;
        let audits = self
            .db
            .audit_logs()
            .list_master_mapping_task_histories(&task_ids, &mut NoTransaction)
            .await?;
        let history_groups = group_by_key(audits, |audit| audit.resource_id.clone().unwrap_or_default());
        let mut lookups = Vec::new();
        let mut external_ids = Vec::with_capacity(ordered_tasks.len());
        for task in &ordered_tasks {
            let Some(snapshot) = snapshot_map.get(&task.source_snapshot_id.to_string()) else {
                external_ids.push(Err("映射任务引用的来源快照不存在".to_string()));
                continue;
            };
            match task
                .mapping_type
                .snapshot_external_identity(&snapshot.normalized_snapshot)
                .map(|identity| identity.into_string())
                .map_err(snapshot_identity_error)
            {
                Ok(external_id) => {
                    if let Some(registration) = task.mapping_type.target_registration() {
                        lookups.push((
                            snapshot.source_system_id.clone(),
                            registration.object_type,
                            ExternalIdentityMap::external_id_key(&external_id),
                        ));
                    }
                    external_ids.push(Ok(external_id));
                }
                Err(error) => external_ids.push(Err(error.to_string())),
            }
        }
        let maps = self
            .db
            .external_identity_maps()
            .find_maps_by_identities(&lookups, &mut NoTransaction)
            .await?;
        let map_ids = maps.iter().map(|map| map.base.id.clone()).collect::<Vec<_>>();
        let targets = self
            .db
            .external_identity_targets()
            .list_targets_for_maps(&map_ids, &mut NoTransaction)
            .await?;
        let target_groups = group_by_key(targets, |target| target.external_identity_map_id.to_string());
        let mut candidate_cache: Vec<(MappingTaskType, Vec<MappingCandidateTargetView>)> = Vec::new();
        let mut items = Vec::with_capacity(ordered_tasks.len());
        for (task, external) in ordered_tasks.into_iter().zip(external_ids) {
            let snapshot = snapshot_map
                .get(&task.source_snapshot_id.to_string())
                .cloned()
                .ok_or_else(|| Error::Internal("映射任务引用的来源快照不存在".to_string()))?;
            let work_item = match work_groups.get(&task.base.id) {
                None => None,
                Some(group) if group.len() == 1 => Some(group[0].clone()),
                Some(_) => {
                    return Err(Error::Internal(
                        "同一映射任务存在多个正式任务，责任事实不唯一".to_string(),
                    ));
                }
            };
            let source = source_map
                .get(&snapshot.source_system_id.to_string())
                .cloned()
                .ok_or_else(|| Error::Internal("映射任务引用的来源商城不存在".to_string()))?;
            let eligible = work_item.is_some();
            let stage_active = source.system_type == SourceSystemType::Mall
                && source.stable.status == SourceSystemStatus::Active
                && source.mall_sync_stage == Some(MallSyncStage::FirstPhaseMallOwned);
            let candidate_targets = if eligible && stage_active {
                if let Some(cached) = candidate_cache
                    .iter()
                    .find(|(cached_type, _)| *cached_type == task.mapping_type)
                {
                    cached.1.clone()
                } else {
                    let candidates = self.mapping_candidates(task.mapping_type, actor.id()).await?;
                    candidate_cache.push((task.mapping_type, candidates.clone()));
                    candidates
                }
            } else {
                Vec::new()
            };
            let (external_identity_map_id, current_targets, lineage_error) = match external {
                Ok(external_id) => {
                    let key = ExternalIdentityMap::external_id_key(&external_id);
                    let wanted = task
                        .mapping_type
                        .target_registration()
                        .map(|registration| registration.object_type);
                    let map = wanted.and_then(|object_type| {
                        maps.iter().find(|map| {
                            map.source_system_id.to_string() == snapshot.source_system_id.to_string()
                                && map.object_type == object_type
                                && map.external_id_key == key
                        })
                    });
                    match map {
                        None => (None, Vec::new(), None),
                        Some(map) => {
                            let views = target_groups
                                .get(&map.base.id)
                                .map(|group| current_target_views(group))
                                .unwrap_or_default();
                            (Some(map.base.id.clone()), views, None)
                        }
                    }
                }
                Err(error) => (None, Vec::new(), Some(error)),
            };
            let facts = MappingTaskFacts {
                snapshot,
                work_item,
                source,
                candidate_targets,
                external_identity_map_id,
                current_targets,
                lineage_error,
                reapply_operation: reapply_map
                    .get(&task.base.id)
                    .cloned()
                    .map(super::dto::ReapplyOperationView::from),
                resolution_history: history_groups
                    .get(&task.base.id)
                    .map(|group| history_views(group))
                    .unwrap_or_default(),
            };
            items.push(assemble_mapping_task_view(task, facts, actor));
        }

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{current_target_views, group_by_key, history_views, index_by_key};

    #[test]
    fn index_by_key_overwrites_and_group_preserves_order() {
        let indexed = index_by_key(
            vec![("b".to_string(), 1), ("a".to_string(), 2), ("b".to_string(), 3)],
            |(key, _)| key.clone(),
        );
        assert_eq!(indexed.len(), 2);
        assert_eq!(indexed["b"].1, 3);
        let grouped = group_by_key(
            vec!["t-1".to_string(), "t-2".to_string(), "t-1".to_string()],
            |id| id.clone(),
        );
        assert_eq!(grouped["t-1"], vec!["t-1".to_string(), "t-1".to_string()]);
        assert!(!grouped.contains_key("missing"));
    }

    #[test]
    fn history_and_target_views_handle_empty() {
        assert!(history_views(&[]).is_empty());
        assert!(current_target_views(&[]).is_empty());
    }
}
