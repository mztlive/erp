use application_core::{QueryIds, normalized_text, page_or_default, page_size_or_default};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::common::{PageParams, non_blank, normalize_sort, reject_me_ids};
use super::error_task::ActionBlockerView;
use super::task_decision::{
    ControlledEvidenceRef, ReconciliationReasonRegistryView, ResolutionEvidencePolicyView,
};
use crate::Result;
use crate::entity::integration_ops::{
    ReconciliationDifference, ReconciliationDifferenceResolution, ResolutionAction, ResultingStatus,
};

/// `reconciliation_difference` 列表允许的排序字段白名单（仅差异发现时间）。
pub(crate) const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at"];

/// 对账差异登记请求（创建后不可修改，不设软删除）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateDifferenceRequest {
    /// 差异对象类型（如商城订单、供应商订单、销售单等）。
    #[validate(
        custom(function = "non_blank", message = "差异对象类型不能为空"),
        length(max = 64, message = "差异对象类型过长")
    )]
    pub business_object_type: String,
    /// 差异对象 ID。
    #[validate(
        custom(function = "non_blank", message = "差异对象ID不能为空"),
        length(max = 128, message = "差异对象ID过长")
    )]
    pub business_object_id: String,
    /// 差异分类。
    #[validate(
        custom(function = "non_blank", message = "差异分类不能为空"),
        length(max = 64, message = "差异分类过长")
    )]
    pub difference_type: String,
    /// 左侧不可变证据引用；两侧至少其一。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用；两侧至少其一。
    #[validate(length(max = 512, message = "证据引用过长"))]
    pub right_fact_reference: Option<String>,
    /// 创建时明确指定的当前责任人。
    #[validate(length(min = 1, max = 128, message = "责任人不能为空或过长"))]
    pub owner_user_id: String,
}

/// 对账差异列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DifferenceListParams {
    /// 编号、业务对象、事件或差异摘要的字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,

    /// 差异对象类型筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 差异分类筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界（秒级时间戳，含）。
    pub created_at_from: Option<i64>,
    /// 发现时间上界（秒级时间戳，含）。
    pub created_at_to: Option<i64>,
    /// 当前处理人稳定 ID 列表。
    pub handler_user_ids: Option<QueryIds>,
    /// 历史处理人稳定 ID 列表（处理记录 `handled_by`）。
    pub operator_user_ids: Option<QueryIds>,
    /// 当前处理人所属内部组织。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 跨页必须携带的范围版本。
    pub scope_version: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的对账差异列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DifferenceListQuery {
    /// 编号、业务对象、事件或差异摘要的字面量关键词。
    pub q: Option<String>,

    /// 差异对象类型筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID 筛选。
    pub business_object_id: Option<String>,
    /// 差异分类筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界。
    pub created_at_from: Option<i64>,
    /// 发现时间上界。
    pub created_at_to: Option<i64>,
    /// 当前处理人稳定 ID。
    pub handler_user_ids: Vec<String>,
    /// 历史处理人稳定 ID。
    pub operator_user_ids: Vec<String>,
    /// 当前处理人所属内部组织。
    pub org_unit_ids: Vec<String>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: bool,
    /// 跨页范围版本。
    pub scope_version: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl DifferenceListParams {
    /// 归一化对账差异列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<DifferenceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DIFFERENCE_SORT_FIELDS)?;
        if self.scope_version.as_ref().is_some_and(|version| version.is_empty() || version.len() > 256) {
            return Err(crate::Error::ValidationError("范围版本非法".into()));
        }
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        let handler_user_ids = self.handler_user_ids.as_ref().map(QueryIds::as_slice).unwrap_or(&[]).to_vec();
        let operator_user_ids =
            self.operator_user_ids.as_ref().map(QueryIds::as_slice).unwrap_or(&[]).to_vec();
        reject_me_ids(&handler_user_ids, "当前处理人")?;
        reject_me_ids(&operator_user_ids, "历史处理人")?;
        Ok(DifferenceListQuery {
            q: normalized_text(self.q.as_deref()),
            business_object_type: normalized_text(self.business_object_type.as_deref()),
            business_object_id: normalized_text(self.business_object_id.as_deref()),
            difference_type: normalized_text(self.difference_type.as_deref()),
            created_at_from: self.created_at_from,
            created_at_to: self.created_at_to,
            handler_user_ids,
            operator_user_ids,
            org_unit_ids: self.org_unit_ids.as_ref().map(QueryIds::as_slice).unwrap_or(&[]).to_vec(),
            include_descendants: self.include_descendants.unwrap_or(false),
            scope_version: self.scope_version.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 对账差异列表响应视图（`status` 由最后一条处理记录派生；无处理记录时为 `None`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceView {
    /// 实体主键。
    pub id: String,
    /// 差异对象类型。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
    /// 派生处理状态（`None` 表示尚无处理记录）。
    pub status: Option<ResultingStatus>,
    /// 最新追加式决定序号；初始为 `0`。
    pub version: u64,
    /// 差异发现时间（秒级时间戳）。
    pub created_at: u64,
    /// 当前处理人。
    pub owner_user_id: String,
}

/// 带范围版本的对账差异列表响应。
#[derive(Debug, Clone, Serialize)]
pub struct DifferenceListView {
    /// 分页结果。
    #[serde(flatten)]
    pub data: super::PageView<DifferenceView>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前范围口径摘要。
    pub scope_summary: &'static str,
    /// 当前处理人权威来源。
    pub ownership_basis: &'static str,
}

/// 差异处理记录视图（不可变追加历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolutionView {
    /// 实体主键。
    pub id: String,
    /// 递增处理序号。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间（秒级时间戳）。
    pub handled_at: i64,
}

impl From<ReconciliationDifferenceResolution> for ResolutionView {
    /// 从处理记录实体构造视图。
    ///
    /// # 参数
    /// * `resolution` - 处理记录实体
    ///
    /// # 返回
    /// 返回处理记录视图。
    fn from(resolution: ReconciliationDifferenceResolution) -> Self {
        Self {
            id: resolution.base.id,
            resolution_no: resolution.resolution_no,
            resolution_action: resolution.resolution_action,
            resulting_status: resolution.resulting_status,
            evidence_reference: resolution.evidence_reference,
            handled_by: resolution.handled_by,
            handled_at: resolution.handled_at.unix_secs(),
        }
    }
}

/// 对账差异详情响应视图（差异字段 + 全部处理记录时间线）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DifferenceDetailView {
    /// 差异字段（扁平展开）。
    #[serde(flatten)]
    pub difference: DifferenceView,
    /// 处理记录时间线（按处理序号升序，不可变）。
    pub resolutions: Vec<ResolutionView>,
    /// 服务端按任务关联与当前差异状态开放的动作。
    pub allowed_actions: Vec<String>,
    /// 当前阻断原因。
    pub action_blockers: Vec<ActionBlockerView>,
    /// 服务端发现并重验的受控证据。
    pub linked_evidence: Vec<ControlledEvidenceRef>,
    /// 有正式任务时使用的固定终态证据策略。
    pub resolution_evidence_policy: Option<ResolutionEvidencePolicyView>,
    /// 无正式任务时使用的固定直接对账原因注册表。
    pub reconciliation_reason_registry: Option<ReconciliationReasonRegistryView>,
}

impl From<ReconciliationDifference> for DifferenceView {
    /// 从差异实体构造视图（无处理记录时状态为 `None`）。
    ///
    /// # 参数
    /// * `difference` - 差异实体
    ///
    /// # 返回
    /// 返回差异视图。
    fn from(difference: ReconciliationDifference) -> Self {
        Self {
            id: difference.base.id,
            business_object_type: difference.business_object_type,
            business_object_id: difference.business_object_id,
            difference_type: difference.difference_type,
            left_fact_reference: difference.left_fact_reference,
            right_fact_reference: difference.right_fact_reference,
            status: None,
            version: 0,
            created_at: difference.base.created_at,
            owner_user_id: difference.owner_user_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::DifferenceListParams;

    #[test]
    fn difference_list_params_normalize_and_reject_unbounded_page_size() {
        let params = DifferenceListParams {
            q: Some("  needle.[x]  ".to_string()),
            business_object_type: Some(" mall_order ".to_string()),
            business_object_id: None,
            difference_type: None,
            created_at_from: None,
            created_at_to: None,
            handler_user_ids: None,
            operator_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            scope_version: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let params = DifferenceListParams {
            q: Some("  needle.[x]  ".to_string()),
            business_object_type: Some(" mall_order ".to_string()),
            business_object_id: Some("MO-1".to_string()),
            difference_type: Some("amount_mismatch".to_string()),
            created_at_from: Some(1_700_000_000),
            created_at_to: Some(1_700_000_100),
            handler_user_ids: None,
            operator_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            scope_version: None,
            page: Some(3),
            page_size: Some(10),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("desc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.q.as_deref(), Some("needle.[x]"));
        assert_eq!(query.business_object_type.as_deref(), Some("mall_order"));
        assert_eq!(query.business_object_id.as_deref(), Some("MO-1"));
        assert_eq!(query.created_at_to, Some(1_700_000_100));
        assert_eq!(query.paging.page, 3);
        assert_eq!(query.paging.page_size, 10);
    }

    #[test]
    fn difference_list_params_reject_me_as_person_id() {
        let handlers = DifferenceListParams {
            handler_user_ids: Some(serde_json::from_value(serde_json::json!("me")).unwrap()),
            ..DifferenceListParams::default()
        };
        assert!(handlers.normalized().is_err());
        let operators = DifferenceListParams {
            operator_user_ids: Some(serde_json::from_value(serde_json::json!("ME")).unwrap()),
            ..DifferenceListParams::default()
        };
        assert!(operators.normalized().is_err());
    }

    #[test]
    fn difference_view_projects_current_handler() {
        use crate::entity::integration_ops::{ReconciliationDifference, ReconciliationDifferenceId};
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-view"),
            crate::entity::integration_ops::reconciliation_difference::tests::difference_data(),
        )
        .unwrap();
        let view = super::DifferenceView::from(difference);
        assert_eq!(view.owner_user_id, "user-1");
        assert_eq!(view.id, "diff-view");
    }
}
