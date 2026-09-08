//! 运行实例列表、详情、历史与恢复选项查询。

use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItem;
use crate::repository::approval_integration::{
    ApprovalRuntimeReadRepository, ApprovalRuntimeReadRow, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};
use crate::repository::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListProjection, ApprovalInstanceListView,
    ApprovalInstanceSummary, ApprovalInstanceTextQuery,
};
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use bpm::engine::TaskIntent;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use erp_core::common::time::Instant;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::super::apply_plan::PlannedWrites;
use super::super::runtime_history::{history_item_from_execution, history_page_from, RuntimeHistoryPage};
use super::super::runtime_query::{
    recovery_options_for, RuntimeInstanceListView, RuntimeInstanceStatusFilter, RuntimeRecoveryAction,
};
use super::super::view::OpenTaskSummary;
use super::hidden_not_found;
use super::read_auth::{
    current_execution_matches_instance, ensure_mine_page_integrity, management_runtime_read_allowed,
    mine_execution_ids, mine_instance_ids, mine_runtime_chain_matches, ordinary_runtime_read_allowed,
    started_runtime_read_allowed, task_proves_current_responsibility, unique_by_id,
    RuntimeReadAuthorizationFacts, RuntimeReadSubject,
};
use super::ApprovalRuntimeService;
use crate::error::{Error, Result};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::policy::{policy_of, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES};
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::scope::definition_management_visibility;
use crate::service::approval::{approval_actor_is_active, approval_document_read_scope};
use application_core::AuditActor;

/// 实例列表默认页大小。
const DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT: u32 = 20;
/// 实例列表最大页大小。
const MAX_RUNTIME_INSTANCE_LIST_LIMIT: u32 = 100;
/// 实例列表检索串与游标 ID 最大字符数。
const RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN: usize = 128;

/// 实例列表查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInstanceListQuery {
    /// 固定视图。
    pub view: RuntimeInstanceListView,
    /// 可选单据类型稳定码。
    pub document_type: Option<String>,
    /// 可选状态。
    pub status: Option<RuntimeInstanceStatusFilter>,
    /// 当前视图的稳定游标。
    pub cursor: Option<RuntimeInstanceListCursor>,
    /// 页大小。
    pub limit: u32,
    /// 可选字面量检索；空表示不按关键词过滤。
    pub query: Option<String>,
}

impl RuntimeInstanceListQuery {
    /// 由协议输入形成规范化查询。
    ///
    /// # 参数
    /// * `view` - 固定查询视图
    /// * `document_type` - 可选单据类型稳定码
    /// * `status` - 可选实例状态
    /// * `cursor` - HTTP 层已解码的稳定游标
    /// * `limit` - 可选页大小；省略时使用 20
    /// * `query` - 可选字面量检索串
    ///
    /// # 返回
    /// 返回已规范化并通过完整边界校验的查询。
    ///
    /// # 错误
    /// view/status、document_type、limit、cursor 或检索串不符合合同时返回校验错误。
    pub fn prepare(
        view: RuntimeInstanceListView,
        document_type: Option<String>,
        status: Option<RuntimeInstanceStatusFilter>,
        cursor: Option<RuntimeInstanceListCursor>,
        limit: Option<u32>,
        query: Option<String>,
    ) -> Result<Self> {
        let prepared = Self {
            view,
            document_type,
            status,
            cursor: cursor.map(Self::prepare_cursor),
            limit: limit.unwrap_or(DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT),
            query: query
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };
        prepared.validate()?;
        Ok(prepared)
    }

    /// 校验规范化查询的全部纯输入合同。
    ///
    /// # 返回
    /// 所有合同成立时返回 `Ok(())`。
    ///
    /// # 错误
    /// view/status、document_type、limit、cursor 或检索串不符合合同时返回校验错误。
    pub fn validate(&self) -> Result<()> {
        self.validate_view_status()?;
        if let Some(document_type) = self.document_type.as_deref() {
            parse_document_type(document_type)?;
        }
        if !(1..=MAX_RUNTIME_INSTANCE_LIST_LIMIT).contains(&self.limit) {
            return Err(Error::ValidationError(format!(
                "limit 必须在 1 到 {MAX_RUNTIME_INSTANCE_LIST_LIMIT} 之间"
            )));
        }
        self.validate_cursor()?;
        self.validate_query()
    }

    /// trim 游标 ID；排序时间的完整 `i64` 定义域保持不变。
    fn prepare_cursor(mut cursor: RuntimeInstanceListCursor) -> RuntimeInstanceListCursor {
        cursor.id = cursor.id.trim().to_string();
        cursor
    }

    /// 校验固定视图允许的状态集合。
    fn validate_view_status(&self) -> Result<()> {
        match (self.view, self.status) {
            (RuntimeInstanceListView::Mine, None | Some(RuntimeInstanceStatusFilter::Running))
            | (RuntimeInstanceListView::Blocked, None | Some(RuntimeInstanceStatusFilter::Blocked))
            | (RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed, _) => Ok(()),
            (RuntimeInstanceListView::Mine, _) => Err(Error::ValidationError(
                "mine 只接受省略 status 或 status=RUNNING".to_string(),
            )),
            (RuntimeInstanceListView::Blocked, _) => Err(Error::ValidationError(
                "blocked 只接受省略 status 或 status=BLOCKED".to_string(),
            )),
        }
    }

    /// 校验游标 ID 为已 trim 的有界稳定标识。
    fn validate_cursor(&self) -> Result<()> {
        let Some(cursor) = self.cursor.as_ref() else {
            return Ok(());
        };
        let id = cursor.id.as_str();
        if id.is_empty() || id != id.trim() {
            return Err(Error::ValidationError("cursor id 不能为空".to_string()));
        }
        if id.chars().count() > RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN {
            return Err(Error::ValidationError(format!(
                "cursor id 不能超过 {RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN} 个字符"
            )));
        }
        Ok(())
    }

    /// 校验字面量检索串已规范化且长度有界。
    fn validate_query(&self) -> Result<()> {
        let Some(query) = self.query.as_deref() else {
            return Ok(());
        };
        if query.is_empty() || query != query.trim() {
            return Err(Error::ValidationError("q 必须是非空规范化文本".to_string()));
        }
        if query.chars().count() > RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN {
            return Err(Error::ValidationError(format!(
                "q 不能超过 {RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN} 个字符"
            )));
        }
        Ok(())
    }
}

/// 实例列表稳定游标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListCursor {
    /// 当前视图排序时间。
    pub sort_time: i64,
    /// 并列时的实例主键。
    pub id: String,
}

#[cfg(test)]
mod runtime_instance_list_query_tests {
    use crate::entity::document_registry::DocumentType;

    use super::{
        RuntimeInstanceListCursor, RuntimeInstanceListQuery, RuntimeInstanceListView,
        RuntimeInstanceStatusFilter, DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT, MAX_RUNTIME_INSTANCE_LIST_LIMIT,
        RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN,
    };

    fn prepare(
        view: RuntimeInstanceListView,
        status: Option<RuntimeInstanceStatusFilter>,
    ) -> crate::error::Result<RuntimeInstanceListQuery> {
        RuntimeInstanceListQuery::prepare(view, None, status, None, None, None)
    }

    /// 四种视图逐一覆盖省略状态及全部固定状态。
    #[test]
    fn view_status_matrix_is_complete() {
        let views = [
            RuntimeInstanceListView::Mine,
            RuntimeInstanceListView::Blocked,
            RuntimeInstanceListView::Started,
            RuntimeInstanceListView::Managed,
        ];
        let statuses = [
            None,
            Some(RuntimeInstanceStatusFilter::Running),
            Some(RuntimeInstanceStatusFilter::Approved),
            Some(RuntimeInstanceStatusFilter::Cancelled),
            Some(RuntimeInstanceStatusFilter::Blocked),
        ];
        for view in views {
            for status in statuses {
                let expected = match view {
                    RuntimeInstanceListView::Mine => {
                        matches!(status, None | Some(RuntimeInstanceStatusFilter::Running))
                    }
                    RuntimeInstanceListView::Blocked => {
                        matches!(status, None | Some(RuntimeInstanceStatusFilter::Blocked))
                    }
                    RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed => true,
                };
                assert_eq!(prepare(view, status).is_ok(), expected, "{view:?} {status:?}");
            }
        }
    }

    /// limit 省略、两端点及越界值必须得到唯一结果。
    #[test]
    fn limit_defaults_and_rejects_outside_closed_range() {
        let default = prepare(RuntimeInstanceListView::Managed, None).expect("默认 limit");
        assert_eq!(default.limit, DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT);
        for limit in [1, MAX_RUNTIME_INSTANCE_LIST_LIMIT] {
            let query = RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                None,
                Some(limit),
                None,
            )
            .expect("闭区间端点");
            assert_eq!(query.limit, limit);
        }
        for limit in [0, MAX_RUNTIME_INSTANCE_LIST_LIMIT + 1] {
            assert!(RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                None,
                Some(limit),
                None,
            )
            .is_err());
        }
    }

    /// document_type 只接受注册表中的精确稳定码，prepare 与直接 validate 必须同样失败关闭。
    #[test]
    fn document_type_requires_an_exact_registered_code() {
        let registered = DocumentType::SalesOrder.as_str();
        let prepared = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Managed,
            Some(registered.to_string()),
            None,
            None,
            None,
            None,
        )
        .expect("精确登记码");
        assert_eq!(prepared.document_type.as_deref(), Some(registered));

        for document_type in ["", "   ", "unknown", "SALES_ORDER", "sales_order "] {
            assert!(
                RuntimeInstanceListQuery::prepare(
                    RuntimeInstanceListView::Managed,
                    Some(document_type.to_string()),
                    None,
                    None,
                    None,
                    None,
                )
                .is_err(),
                "prepare 必须拒绝 {document_type:?}"
            );

            let mut direct = prepared.clone();
            direct.document_type = Some(document_type.to_string());
            assert!(direct.validate().is_err(), "validate 必须拒绝 {document_type:?}");
        }
    }

    /// cursor 保留完整 i64 时间域，ID 则 trim、非空且最多 128 字符。
    #[test]
    fn cursor_prepares_id_and_preserves_i64_time_domain() {
        for sort_time in [i64::MIN, i64::MAX] {
            let query = RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                Some(RuntimeInstanceListCursor {
                    sort_time,
                    id: "  inst-1  ".to_string(),
                }),
                None,
                None,
            )
            .expect("合法 i64 时间与可规范化 ID");
            let cursor = query.cursor.expect("游标");
            assert_eq!(cursor.sort_time, sort_time);
            assert_eq!(cursor.id, "inst-1");
        }
        let max_id = "a".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN);
        assert!(RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Managed,
            None,
            None,
            Some(RuntimeInstanceListCursor {
                sort_time: 0,
                id: max_id,
            }),
            None,
            None,
        )
        .is_ok());
        for id in [
            "   ".to_string(),
            "a".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN + 1),
        ] {
            assert!(RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                Some(RuntimeInstanceListCursor { sort_time: 0, id }),
                None,
                None,
            )
            .is_err());
        }
    }

    /// q 空白归 None，文本 trim，字符上限不得按字节数误判。
    #[test]
    fn query_text_is_trimmed_and_character_bounded() {
        let blank = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("   ".to_string()),
        )
        .expect("空白 q");
        assert_eq!(blank.query, None);

        let trimmed = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("  SO-1  ".to_string()),
        )
        .expect("trim q");
        assert_eq!(trimmed.query.as_deref(), Some("SO-1"));

        let max = "界".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN);
        assert!(RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some(max),
        )
        .is_ok());
        assert!(RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("界".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN + 1)),
        )
        .is_err());
    }
}

/// 实例列表行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListItem {
    /// 实例 ID。
    pub instance_id: String,
    /// 实例状态。
    pub status: String,
    /// 轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node_key: Option<String>,
    /// 当前节点名。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee_participant_id: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 被审批单据类型稳定码。
    pub document_type: Option<String>,
    /// 被审批业务对象 ID。
    pub document_id: Option<String>,
    /// 被审批单据业务编号。
    pub document_label: Option<String>,
    /// 与实例、对象及提交版本匹配的启动快照金额；未知或非金额审批为 None。
    #[serde(default)]
    pub total_amount: Option<erp_core::money::Amount>,
    /// 审批定义业务版本。
    pub process_version: Option<u32>,
    /// 发起时间。
    pub started_at: Option<i64>,
    /// 最近驳回原因摘要。
    pub latest_rejection_summary: Option<String>,
}

/// 实例列表页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListPage {
    /// 当前页。
    pub items: Vec<RuntimeInstanceListItem>,
    /// 当前过滤条件下的完整数量。
    pub total: u64,
    /// 下一页稳定游标。
    pub next_cursor: Option<RuntimeInstanceListCursor>,
}

/// 恢复选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRecoveryOptionsView {
    /// 实例 ID。
    pub instance_id: String,
    /// 允许的恢复动作。
    pub actions: Vec<RuntimeRecoveryAction>,
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 按固定 view 查询实例摘要。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `query` - 已规范化查询
    ///
    /// # 错误
    /// view/status 非法或仓储失败时返回错误。
    pub async fn instance_list(
        &self,
        actor: &AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        query.validate()?;
        self.ensure_active_instance_reader(actor).await?;
        match query.view {
            RuntimeInstanceListView::Mine => self.list_mine(actor, &query).await,
            _ => self.list_managed_or_started(actor, &query).await,
        }
    }

    /// 读取实例详情与最近执行。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    ///
    /// # 返回
    /// 返回实例状态与当前执行投影。
    ///
    /// # 错误
    /// 不存在或无权时不泄露存在性。
    pub async fn instance_detail(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeInstanceListItem> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_ordinary_runtime_read(actor, &subject).await?;
        let instance = subject.instance;
        let execution = subject.current_execution;
        Ok(item_from_instance_id(
            instance_id,
            instance.status.as_str(),
            instance.current_round_no,
            execution.as_ref().map(|item| item.node_key.clone()),
            execution.as_ref().map(|item| item.node_name.clone()),
            execution
                .as_ref()
                .map(|item| item.assignee_participant_id.as_str().to_string()),
        ))
    }

    /// 读取实例执行历史。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    /// * `after_execution_no` - 上一页最后执行序号；首页为空
    /// * `limit` - 页大小
    ///
    /// # 返回
    /// 返回按 `execution_no` 升序的历史页，字段对齐审批 Tab。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    pub async fn instance_history(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        after_execution_no: Option<u32>,
        limit: u32,
    ) -> Result<RuntimeHistoryPage> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_ordinary_runtime_read(actor, &subject).await?;
        let fetch_limit = limit.saturating_add(1);
        let rows = self
            .db
            .bpm_workflow()
            .list_execution_history(
                &bpm::ids::ApprovalProcessInstanceId::new(instance_id),
                after_execution_no,
                fetch_limit,
                &mut NoTransaction,
            )
            .await?;
        let items = rows.iter().map(history_item_from_execution).collect();
        Ok(history_page_from(items, limit))
    }

    /// 返回当前 blocker 的唯一合法恢复动作。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `instance_id` - 审批实例 ID
    ///
    /// # 返回
    /// 返回实例 ID 与当前允许的恢复动作集合。
    ///
    /// # 错误
    /// 实例不存在时不泄露存在性。
    pub async fn recovery_options(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeRecoveryOptionsView> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_management_runtime_read(actor, &subject).await?;
        let instance = subject.instance;
        let blocked = instance.status == bpm::model::types::ApprovalProcessInstanceStatus::Blocked;
        Ok(RuntimeRecoveryOptionsView {
            instance_id: instance_id.to_string(),
            actions: recovery_options_for(blocked, instance.blocker_code),
        })
    }

    /// 重验当前读取主体仍为有效账号；失效时隐藏目标实例存在性。
    ///
    /// # 参数
    /// * `actor` - Handler 已认证但状态可能已经变化的账号快照
    ///
    /// # 返回
    /// 账号仍存在、类型一致且启用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 账号失效时返回隐藏存在性的 NotFound；仓储失败时传播基础设施错误。
    async fn ensure_active_instance_reader(&self, actor: &AuditActor) -> Result<()> {
        if approval_actor_is_active(&self.auth, actor).await? {
            return Ok(());
        }
        Err(hidden_not_found())
    }

    /// 加载实例、当前执行与唯一冻结快照，并校验运行主体三元组。
    async fn load_runtime_read_subject(&self, instance_id: &str) -> Result<RuntimeReadSubject> {
        let instance_id = ApprovalProcessInstanceId::new(instance_id);
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(instance_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let document_type = crate::entity::approval_integration::document_type_from_subject_kind(
            instance.subject.subject_kind(),
        )
        .map_err(|_| hidden_not_found())?;
        adapter_spec_of(document_type)?;
        if instance.process_kind != process_kind_of(document_type)
            || snapshot
                .ensure_matches_runtime_subject(
                    document_type,
                    instance.subject.subject_id(),
                    instance.subject_version,
                )
                .is_err()
        {
            return Err(hidden_not_found());
        }
        let current_execution = self
            .db
            .bpm_workflow()
            .find_current_execution(&instance_id, &mut NoTransaction)
            .await?;
        if !current_execution_matches_instance(&instance, current_execution.as_ref()) {
            return Err(hidden_not_found());
        }
        Ok(RuntimeReadSubject {
            instance,
            current_execution,
            snapshot,
            document_type,
        })
    }

    /// 校验普通详情/历史读取的三条互斥授权来源。
    async fn ensure_ordinary_runtime_read(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<()> {
        let initiator = subject.instance.started_by.as_str() == actor.id();
        if ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator,
            current_responsibility: false,
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
        }) {
            return Ok(());
        }
        if self.current_runtime_responsibility(actor, subject).await? {
            return Ok(());
        }
        adapter_spec_of(subject.document_type)?;
        let scope = approval_document_read_scope(&self.auth, actor, subject.document_type).await?;
        let facts = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: false,
            current_responsibility: false,
            object_readable: !scope.is_empty(),
            scope_covers: scope.covers(&subject.snapshot.payload.responsible_org_id),
            runtime_admin: false,
        };
        if ordinary_runtime_read_allowed(facts) {
            return Ok(());
        }
        Err(hidden_not_found())
    }

    /// 校验恢复选项等管理读取的类型、对象与组织三门。
    async fn ensure_management_runtime_read(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<()> {
        let visibility = definition_management_visibility(&self.auth, actor).await?;
        adapter_spec_of(subject.document_type)?;
        let scope = approval_document_read_scope(&self.auth, actor, subject.document_type).await?;
        let facts = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: subject.instance.started_by.as_str() == actor.id(),
            current_responsibility: false,
            object_readable: !scope.is_empty(),
            scope_covers: scope.covers(&subject.snapshot.payload.responsible_org_id),
            runtime_admin: visibility.runtime_admin_types().contains(&subject.document_type),
        };
        if management_runtime_read_allowed(facts) {
            return Ok(());
        }
        Err(hidden_not_found())
    }

    /// 判断当前执行是否仍有由本人承担的开放审批任务。
    async fn current_runtime_responsibility(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<bool> {
        if subject.instance.status != ApprovalProcessInstanceStatus::Running {
            return Ok(false);
        }
        let Some(execution) = &subject.current_execution else {
            return Ok(false);
        };
        if execution.status != ApprovalNodeExecutionStatus::Active
            || execution.round_no != subject.instance.current_round_no
            || execution.assignee_participant_id.as_str() != actor.id()
        {
            return Ok(false);
        }
        let execution_id = ApprovalNodeExecutionId::new(execution.base.id.clone());
        let tasks = self
            .db
            .work_items()
            .open_approval_tasks_for_execution(&execution_id, &mut NoTransaction)
            .await?;
        if tasks.len() != 1 {
            return Ok(false);
        }
        let owner_role = adapter_spec_of(subject.document_type)?.owner_role;
        Ok(task_proves_current_responsibility(
            &tasks[0],
            execution,
            subject,
            actor.id(),
            owner_role.as_str(),
        ))
    }

    /// 返回由开放审批任务映射的运行实例列表页。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 可选单据类型与页大小
    ///
    /// # 返回
    /// 返回当前账号拥有的开放单据审批任务页。
    ///
    /// # 错误
    /// WorkItem Repository 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 任务类型、开放状态、责任人与可选单据类型全部由 Repository 固定查询封装。
    async fn list_mine(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let document_type = query
            .document_type
            .as_deref()
            .map(parse_document_type)
            .transpose()?;
        let cursor = query
            .cursor
            .as_ref()
            .map(|cursor| {
                if cursor.sort_time < 0 {
                    return Err(Error::ValidationError(
                        "mine cursor sort_time 不能为负数".to_string(),
                    ));
                }
                Ok((cursor.sort_time, cursor.id.as_str()))
            })
            .transpose()?;
        let page = self
            .db
            .work_items()
            .page_open_document_approval_owned_by(
                actor.id(),
                document_type.map(|document_type| document_type.as_str()),
                query.query.as_deref(),
                cursor,
                query.limit,
                &mut NoTransaction,
            )
            .await?;
        ensure_mine_page_integrity(page.integrity_conflicts.len())?;
        let items = self.hydrate_mine_items(actor, page.items).await?;
        let next_cursor = if page.has_more {
            page.next_cursor
                .map(|(sort_time, id)| RuntimeInstanceListCursor { sort_time, id })
        } else {
            None
        };
        Ok(RuntimeInstanceListPage {
            items,
            total: page.total,
            next_cursor,
        })
    }

    /// 批量装载 Mine 页的 execution、instance、summary 与 snapshot，并按原任务
    /// 顺序重建实例行。任一身份链漂移时整页失败关闭，禁止静默丢行后伪造 total。
    async fn hydrate_mine_items(
        &self,
        actor: &AuditActor,
        tasks: Vec<WorkItem>,
    ) -> Result<Vec<RuntimeInstanceListItem>> {
        let execution_ids = mine_execution_ids(&tasks)?;
        let executions = self
            .db
            .bpm_workflow()
            .list_executions_by_ids(&execution_ids, &mut NoTransaction)
            .await?;
        let execution_by_id = unique_by_id(executions, |execution| execution.base.id.clone())?;

        let instance_ids = mine_instance_ids(&execution_ids, &execution_by_id)?;
        let summaries = self
            .db
            .bpm_workflow()
            .list_instance_summaries_by_ids(&instance_ids, &mut NoTransaction)
            .await?;
        let summary_by_id = unique_by_id(summaries, |summary| summary.id.clone())?;
        let instance_id_strings = instance_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let snapshots = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_ids(&instance_id_strings, &mut NoTransaction)
            .await?;
        let snapshot_by_instance = unique_by_id(snapshots, |snapshot| {
            snapshot.approval_process_instance_id.to_string()
        })?;

        tasks
            .into_iter()
            .map(|task| {
                let execution_id = task
                    .approval_node_execution_id
                    .as_ref()
                    .ok_or_else(hidden_not_found)?;
                let execution = execution_by_id
                    .get(execution_id.as_ref())
                    .ok_or_else(hidden_not_found)?;
                let summary = summary_by_id
                    .get(execution.process_instance_id.as_ref())
                    .ok_or_else(hidden_not_found)?;
                let snapshot = snapshot_by_instance.get(summary.id.as_str());
                if !mine_runtime_chain_matches(&task, execution, summary, snapshot, actor.id())? {
                    return Err(hidden_not_found());
                }
                item_from_summary(summary.clone(), snapshot)
            })
            .collect()
    }

    /// 查询本人发起或管理范围内的审批实例。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 已规范化查询，可含字面量检索
    ///
    /// # 返回
    /// 返回 MongoDB 联合不可变快照完成授权过滤、检索、计数与分页后的实例页。
    ///
    /// # 错误
    /// 单据类型未登记或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 检索必须在 MongoDB 内施加。不得先取当前页再内存过滤。
    async fn list_managed_or_started(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let type_scopes = self.runtime_read_type_scopes(actor, query).await?;
        let mut filter = instance_list_filter(actor, query)?;
        filter.limit = query.limit.saturating_add(1);
        let scope = if query.view == RuntimeInstanceListView::Started {
            ApprovalRuntimeReadScope::Started {
                process_kinds: type_scopes.iter().map(|scope| scope.process_kind).collect(),
                submitted_by: actor.id().to_string(),
            }
        } else {
            ApprovalRuntimeReadScope::Managed {
                type_scopes: type_scopes.clone(),
            }
        };
        let mut page = ApprovalRuntimeReadRepository::new(&self.db)
            .search(&filter, &scope, &mut NoTransaction)
            .await?;
        let has_more = page.items.len() > query.limit as usize;
        if has_more {
            page.items.truncate(query.limit as usize);
        }
        let next_cursor = has_more
            .then(|| {
                page.items
                    .last()
                    .map(|row| cursor_from_summary(filter.view, &row.instance))
            })
            .flatten();
        let items = page
            .items
            .into_iter()
            .map(|row| item_from_runtime_read_row(row, actor, query.view, &type_scopes))
            .collect::<Result<Vec<_>>>()?;
        Ok(RuntimeInstanceListPage {
            items,
            total: page.total,
            next_cursor,
        })
    }

    /// 计算 Started 或管理视图可进入 Repository 的固定流程种类。
    ///
    /// # 参数
    /// * `actor` - 当前有效账号
    /// * `query` - 已规范化视图与可选固定单据类型
    ///
    /// # 返回
    /// Started 返回全部必须审批类型或请求类型；Managed/Blocked 返回当前账号具备
    /// `runtime_admin_permission` 的类型交集。
    ///
    /// # 错误
    /// 单据类型、政策或 RBAC 读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// Service 只把已证明的类型集合交给 Repository；空集合固定形成空页。
    async fn runtime_read_type_scopes(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<Vec<ApprovalRuntimeReadTypeScope>> {
        let requested = query
            .document_type
            .as_deref()
            .map(parse_document_type)
            .transpose()?;
        let mut allowed = if query.view == RuntimeInstanceListView::Started {
            process_required_document_types()?
        } else {
            definition_management_visibility(&self.auth, actor)
                .await?
                .runtime_admin_types()
                .to_vec()
        };
        if let Some(requested) = requested {
            allowed.retain(|document_type| *document_type == requested);
        }
        let mut scopes = Vec::new();
        for document_type in allowed {
            adapter_spec_of(document_type)?;
            let organization_ids = if query.view == RuntimeInstanceListView::Started {
                None
            } else {
                let scope = approval_document_read_scope(&self.auth, actor, document_type).await?;
                if scope.is_empty() {
                    continue;
                }
                scope.organization_ids().map(ToOwned::to_owned)
            };
            scopes.push(ApprovalRuntimeReadTypeScope {
                process_kind: process_kind_of(document_type),
                organization_ids,
            });
        }
        Ok(scopes)
    }
}

/// 返回政策矩阵中必须接入审批运行时的固定单据类型。
fn process_required_document_types() -> Result<Vec<DocumentType>> {
    let mut document_types = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        if matches!(
            policy_of(document_type)?,
            DocumentApprovalPolicy::ProcessRequired(_)
        ) {
            document_types.push(document_type);
        }
    }
    Ok(document_types)
}

/// 解析实例列表筛选中的单据类型稳定码。
///
/// # 参数
/// * `code` - 调用方提供的单据类型稳定代码
///
/// # 返回
/// 精确命中登记代码时返回对应单据类型。
///
/// # 错误
/// 未登记代码返回原有校验错误文本。
///
/// # 关键业务约束
/// Service 不裁剪、不接受别名，也不维护第二份代码注册表。
pub(super) fn parse_document_type(code: &str) -> Result<DocumentType> {
    DocumentType::try_from_code(code).map_err(|_| Error::ValidationError(format!("未登记单据类型: {code}")))
}

/// 构造实例列表的授权过滤与稳定游标。
///
/// # 参数
/// * `actor` - 当前账号
/// * `query` - 已规范化列表查询
///
/// # 返回
/// 返回仓储过滤条件；无检索串时 `text_query` 为空。
///
/// # 错误
/// 单据类型未登记时返回校验错误。
///
/// # 关键业务约束
/// `Started` 必须带 `started_by`。检索在仓储内施加，不得先分页再内存过滤。
pub(super) fn instance_list_filter(
    actor: &AuditActor,
    query: &RuntimeInstanceListQuery,
) -> Result<ApprovalInstanceListFilter> {
    let view = match query.view {
        RuntimeInstanceListView::Started => ApprovalInstanceListView::Started,
        RuntimeInstanceListView::Blocked => ApprovalInstanceListView::Blocked,
        _ => ApprovalInstanceListView::Managed,
    };
    let process_kind = query
        .document_type
        .as_deref()
        .map(parse_document_type)
        .transpose()?
        .map(process_kind_of);
    Ok(ApprovalInstanceListFilter {
        view,
        process_kind,
        status: query.status.map(map_status_filter),
        started_by: (query.view == RuntimeInstanceListView::Started).then(|| actor.id().to_string()),
        subject_kind: None,
        subject_ids: None,
        text_query: query
            .query
            .as_ref()
            .map(|text| ApprovalInstanceTextQuery { query: text.clone() }),
        cursor: query
            .cursor
            .as_ref()
            .map(|cursor| crate::repository::bpm::ApprovalInstanceListCursor {
                sort_time: cursor.sort_time,
                id: cursor.id.clone(),
            }),
        limit: query.limit,
    })
}

/// 从当前视图最后一行生成下一页游标。
pub(super) fn cursor_from_summary(
    view: ApprovalInstanceListView,
    row: &ApprovalInstanceSummary,
) -> RuntimeInstanceListCursor {
    let updated_at = i64::try_from(row.updated_at).unwrap_or(i64::MAX);
    let sort_time = match view {
        ApprovalInstanceListView::Started => row.started_at,
        ApprovalInstanceListView::Blocked => row.blocked_at.unwrap_or(updated_at),
        ApprovalInstanceListView::Managed => updated_at,
    };
    RuntimeInstanceListCursor {
        sort_time,
        id: row.id.clone(),
    }
}

/// 映射列表状态过滤。
fn map_status_filter(
    status: RuntimeInstanceStatusFilter,
) -> bpm::model::types::ApprovalProcessInstanceStatus {
    match status {
        RuntimeInstanceStatusFilter::Running => bpm::model::types::ApprovalProcessInstanceStatus::Running,
        RuntimeInstanceStatusFilter::Approved => bpm::model::types::ApprovalProcessInstanceStatus::Approved,
        RuntimeInstanceStatusFilter::Cancelled => bpm::model::types::ApprovalProcessInstanceStatus::Cancelled,
        RuntimeInstanceStatusFilter::Blocked => bpm::model::types::ApprovalProcessInstanceStatus::Blocked,
    }
}

/// 由仓储摘要与启动快照映射列表行。
pub(super) fn item_from_summary(
    row: ApprovalInstanceSummary,
    snapshot: Option<&ApprovalSubjectSnapshot>,
) -> Result<RuntimeInstanceListItem> {
    let document_type =
        crate::entity::approval_integration::document_type_from_subject_kind(row.subject.subject_kind())
            .map_err(|_| hidden_not_found())?;
    let document_id = row.subject.subject_id().to_string();
    if row.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    let snapshot = snapshot
        .filter(|snapshot| snapshot.approval_process_instance_id.as_ref() == row.id.as_str())
        .filter(|snapshot| {
            snapshot
                .ensure_matches_runtime_subject(document_type, &document_id, row.subject_version)
                .is_ok()
        });
    let document_label = snapshot.map(|snapshot| snapshot.payload.document_no.clone());
    let total_amount = snapshot.and_then(|snapshot| snapshot.payload.total_amount);
    Ok(RuntimeInstanceListItem {
        instance_id: row.id,
        status: row.status.as_str().to_string(),
        current_round_no: row.current_round_no,
        current_node_key: row.current_node_key,
        current_node_name: row.current_node_name,
        current_assignee_participant_id: row.current_assignee_participant_id,
        current_assignee_name: row.current_assignee_name,
        document_type: Some(document_type.as_str().to_string()),
        document_id: Some(document_id),
        document_label,
        total_amount,
        process_version: Some(row.definition_version),
        started_at: Some(row.started_at),
        latest_rejection_summary: row.latest_rejection_summary,
    })
}

/// 对聚合页行重新执行对象读取与授权矩阵，然后映射公开列表行。
pub(super) fn item_from_runtime_read_row(
    row: ApprovalRuntimeReadRow,
    actor: &AuditActor,
    view: RuntimeInstanceListView,
    type_scopes: &[ApprovalRuntimeReadTypeScope],
) -> Result<RuntimeInstanceListItem> {
    let document_type = crate::entity::approval_integration::document_type_from_subject_kind(
        row.instance.subject.subject_kind(),
    )
    .map_err(|_| hidden_not_found())?;
    let type_allowed = type_scopes
        .iter()
        .any(|scope| scope.process_kind == row.instance.process_kind);
    let facts = RuntimeReadAuthorizationFacts {
        actor_active: true,
        initiator: row.instance.started_by == actor.id(),
        current_responsibility: false,
        object_readable: type_allowed,
        scope_covers: type_allowed,
        runtime_admin: type_allowed,
    };
    let allowed = match view {
        RuntimeInstanceListView::Started => started_runtime_read_allowed(facts),
        RuntimeInstanceListView::Managed | RuntimeInstanceListView::Blocked => {
            management_runtime_read_allowed(facts)
        }
        RuntimeInstanceListView::Mine => false,
    };
    if !allowed {
        return Err(hidden_not_found());
    }
    if row.instance.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    item_from_summary(row.instance, row.snapshot.as_ref())
}

/// 由实例字段构造列表行。
fn item_from_instance_id(
    instance_id: &str,
    status: &str,
    current_round_no: u32,
    current_node_key: Option<String>,
    current_node_name: Option<String>,
    current_assignee_participant_id: Option<String>,
) -> RuntimeInstanceListItem {
    RuntimeInstanceListItem {
        instance_id: instance_id.to_string(),
        status: status.to_string(),
        current_round_no,
        current_node_key,
        current_node_name,
        current_assignee_participant_id,
        current_assignee_name: None,
        document_type: None,
        document_id: None,
        document_label: None,
        total_amount: None,
        process_version: None,
        started_at: None,
        latest_rejection_summary: None,
    }
}

/// 由决定后的写入构造实例列表投影。
pub(super) fn list_projection_from_writes(
    writes: &PlannedWrites,
    ended_execution_id: &str,
    reject_reason: Option<String>,
    now: Instant,
) -> ApprovalInstanceListProjection {
    let current = writes.created_executions.last();
    ApprovalInstanceListProjection {
        current_node_key: current.map(|item| item.node_key.clone()),
        current_node_name: current.map(|item| item.node_name.clone()),
        current_assignee_participant_id: current
            .map(|item| item.assignee_participant_id.as_str().to_string()),
        current_assignee_name: current.map(|item| item.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: reject_reason.as_ref().map(|_| ended_execution_id.to_string()),
        latest_rejection_summary: reject_reason,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// 视图中的首个新建开放任务摘要。
pub(super) fn first_open_task(writes: &PlannedWrites, new_task_ids: &[String]) -> Option<OpenTaskSummary> {
    let intent = writes.create_tasks.first()?;
    let task_id = new_task_ids.first()?;
    let TaskIntent::HumanTaskRequested { assignee, .. } = intent else {
        return None;
    };
    Some(OpenTaskSummary {
        work_item_id: task_id.clone(),
        task_version: "1".to_string(),
        owner_user_id: assignee.as_str().to_string(),
    })
}
