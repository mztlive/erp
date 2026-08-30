//! 域 D02 `document_registry` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。

use entities::document_registry::{
    BusinessDocument, BusinessDocumentId, DocumentParticipant, DocumentParticipantData, DocumentRelation,
    DocumentRelationData, DocumentRelationType, DocumentType, ParticipantRole, WorkflowAction,
    WorkflowActionData, WorkflowActionType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 单据注册列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const BUSINESS_DOCUMENT_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];
/// 工作流动作列表允许的排序字段白名单。
pub(crate) const WORKFLOW_ACTION_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空编号需要按「空白视为空」拒绝，落入 HTTP 400）。
use crate::query::non_blank;

/// 单据注册创建请求（HTTP 契约：`{ id?, document_type, document_no }`）。
///
/// `id` 可选：强类型单据域正式化时携带自己的 `document_id` 作为幂等键
/// （同一注册身份 + 同一 ID 重复登记幂等成功）；缺省由服务端生成。
/// `external_identity_map_id` 可选：外部来源单据登记时携带来源身份映射
/// （跨域校验走 D01 `external_identity_maps` 仓储，仅读取）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RegisterBusinessDocumentRequest {
    /// 单据注册 ID（强类型单据域正式化时提供，作为幂等键）。
    #[validate(length(max = 64, message = "单据ID过长"))]
    pub id: Option<String>,
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号。
    #[validate(custom(function = "non_blank", message = "单据编号不能为空"))]
    pub document_no: String,
    /// 外部来源身份映射 ID（D01）；提供时必须已登记。
    pub external_identity_map_id: Option<String>,
}

/// 单据注册响应视图（契约形状：`id`/`document_type`/`document_no`/`formalized_at`，
/// 另附 `version` 供前端乐观锁更新回传）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BusinessDocumentView {
    /// 实体主键。
    pub id: String,
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号。
    pub document_no: String,
    /// 首次正式化时间（秒级时间戳）。
    pub formalized_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<BusinessDocument> for BusinessDocumentView {
    /// 从实体构造响应视图。
    fn from(doc: BusinessDocument) -> Self {
        Self {
            id: doc.base.id,
            document_type: doc.document_type,
            document_no: doc.document_no,
            formalized_at: doc.formalized_at.map(|instant| instant.unix_secs() as u64),
            version: doc.base.version,
            created_at: doc.base.created_at,
        }
    }
}

/// 单据注册列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BusinessDocumentListParams {
    /// 单据类型筛选。
    pub document_type: Option<DocumentType>,
    /// 单据编号模糊筛选（忽略大小写）。
    pub document_no: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的单据注册列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BusinessDocumentListQuery {
    /// 单据类型筛选。
    pub document_type: Option<DocumentType>,
    /// 单据编号模糊筛选。
    pub document_no: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl BusinessDocumentListParams {
    /// 归一化单据注册列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<BusinessDocumentListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, BUSINESS_DOCUMENT_SORT_FIELDS)?;
        Ok(BusinessDocumentListQuery {
            document_type: self.document_type,
            document_no: normalized_text(self.document_no.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 工作流动作追加请求（HTTP 契约：`{ document_id, action_type, from_status, to_status,
/// comment }`；操作者与责任角色由服务端从鉴权上下文注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AppendWorkflowActionRequest {
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: BusinessDocumentId,
    /// 动作类型。
    pub action_type: WorkflowActionType,
    /// 迁移前状态代码（大写代码形态）。
    #[validate(custom(function = "non_blank", message = "迁移前状态不能为空"))]
    pub from_status: String,
    /// 迁移后状态代码（大写代码形态）。
    #[validate(custom(function = "non_blank", message = "迁移后状态不能为空"))]
    pub to_status: String,
    /// 意见或驳回原因。
    #[validate(length(max = 512, message = "意见过长"))]
    pub comment: Option<String>,
}

impl AppendWorkflowActionRequest {
    /// 转换为实体创建数据（责任角色由服务层按业务规则注入）。
    ///
    /// # 参数
    /// * `actor_role` - 动作发生时的责任角色
    /// * `actor_id` - 实际操作者
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self, actor_role: &str, actor_id: &str) -> WorkflowActionData {
        WorkflowActionData {
            document_id: self.document_id,
            action_type: self.action_type,
            from_status: self.from_status,
            to_status: self.to_status,
            actor_id: actor_id.to_string(),
            actor_role: actor_role.to_string(),
            comment: self.comment,
        }
    }
}

/// 工作流动作响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkflowActionView {
    /// 实体主键。
    pub id: String,
    /// 业务单据 ID。
    pub document_id: String,
    /// 动作类型。
    pub action_type: WorkflowActionType,
    /// 迁移前状态代码。
    pub from_status: String,
    /// 迁移后状态代码。
    pub to_status: String,
    /// 实际操作者。
    pub actor_id: String,
    /// 动作发生时的责任角色。
    pub actor_role: String,
    /// 意见或驳回原因。
    pub comment: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<WorkflowAction> for WorkflowActionView {
    /// 从实体构造响应视图。
    fn from(action: WorkflowAction) -> Self {
        Self {
            id: action.base.id,
            document_id: action.document_id.to_string(),
            action_type: action.action_type,
            from_status: action.from_status,
            to_status: action.to_status,
            actor_id: action.actor_id,
            actor_role: action.actor_role,
            comment: action.comment,
            created_at: action.base.created_at,
        }
    }
}

/// 工作流动作列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkflowActionListParams {
    /// 业务单据 ID 筛选。
    pub document_id: Option<BusinessDocumentId>,
    /// 操作者模糊筛选（忽略大小写）。
    pub actor_id: Option<String>,
    /// 动作类型筛选。
    pub action_type: Option<WorkflowActionType>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的工作流动作列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowActionListQuery {
    /// 业务单据 ID 筛选。
    pub document_id: Option<BusinessDocumentId>,
    /// 操作者模糊筛选。
    pub actor_id: Option<String>,
    /// 动作类型筛选。
    pub action_type: Option<WorkflowActionType>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl WorkflowActionListParams {
    /// 归一化工作流动作列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<WorkflowActionListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, WORKFLOW_ACTION_SORT_FIELDS)?;
        Ok(WorkflowActionListQuery {
            document_id: self.document_id.clone(),
            actor_id: normalized_text(self.actor_id.as_deref()),
            action_type: self.action_type,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 单据关系创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateDocumentRelationRequest {
    /// 变更、退货、退款、冲正或红票单。
    pub from_document_id: BusinessDocumentId,
    /// 被纠正或被引用的原单。
    pub to_document_id: BusinessDocumentId,
    /// 关系类型。
    pub relation_type: DocumentRelationType,
}

impl CreateDocumentRelationRequest {
    /// 转换为实体创建数据。
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self) -> DocumentRelationData {
        DocumentRelationData {
            from_document_id: self.from_document_id,
            to_document_id: self.to_document_id,
            relation_type: self.relation_type,
        }
    }
}

/// 单据关系响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentRelationView {
    /// 实体主键。
    pub id: String,
    /// 变更、退货、退款、冲正或红票单。
    pub from_document_id: String,
    /// 被纠正或被引用的原单。
    pub to_document_id: String,
    /// 关系类型。
    pub relation_type: DocumentRelationType,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<DocumentRelation> for DocumentRelationView {
    /// 从实体构造响应视图。
    fn from(relation: DocumentRelation) -> Self {
        Self {
            id: relation.base.id,
            from_document_id: relation.from_document_id.to_string(),
            to_document_id: relation.to_document_id.to_string(),
            relation_type: relation.relation_type,
            created_at: relation.base.created_at,
        }
    }
}

/// 单据参与人创建请求（HTTP 契约：`{ document_id, participant_role,
/// participant_user_id, participant_name }`；记录人由服务端从鉴权上下文注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateDocumentParticipantRequest {
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: BusinessDocumentId,
    /// 参与角色。
    pub participant_role: ParticipantRole,
    /// 参与人用户 ID。
    #[validate(custom(function = "non_blank", message = "参与人用户ID不能为空"))]
    pub participant_user_id: String,
    /// 参与人名称快照。
    #[validate(custom(function = "non_blank", message = "参与人名称不能为空"))]
    pub participant_name: String,
}

impl CreateDocumentParticipantRequest {
    /// 转换为实体创建数据（记录人由服务层注入）。
    ///
    /// # 参数
    /// * `recorded_by` - 记录人（账号或系统身份）
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self, recorded_by: &str) -> DocumentParticipantData {
        DocumentParticipantData {
            document_id: self.document_id,
            participant_role: self.participant_role,
            participant_user_id: self.participant_user_id,
            participant_name: self.participant_name,
            recorded_by: recorded_by.to_string(),
        }
    }
}

/// 单据参与人响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentParticipantView {
    /// 实体主键。
    pub id: String,
    /// 业务单据 ID。
    pub document_id: String,
    /// 参与角色。
    pub participant_role: ParticipantRole,
    /// 参与人用户 ID。
    pub participant_user_id: String,
    /// 参与人名称快照。
    pub participant_name: String,
    /// 记录人。
    pub recorded_by: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<DocumentParticipant> for DocumentParticipantView {
    /// 从实体构造响应视图。
    fn from(participant: DocumentParticipant) -> Self {
        Self {
            id: participant.base.id,
            document_id: participant.document_id.to_string(),
            participant_role: participant.participant_role,
            participant_user_id: participant.participant_user_id,
            participant_name: participant.participant_name,
            recorded_by: participant.recorded_by,
            created_at: participant.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, BusinessDocumentListParams, CreateDocumentRelationRequest, DocumentRelationType,
        SortDir, WorkflowActionListParams,
    };
    use entities::document_registry::{BusinessDocumentId, DocumentType};
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("document_no".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" updated_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "updated_at"],
        )
        .unwrap();
        assert_eq!(field, "updated_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_and_filters() {
        let params = BusinessDocumentListParams {
            document_type: Some(DocumentType::SalesOrder),
            document_no: Some(" SO-001 ".to_string()),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.document_type, Some(DocumentType::SalesOrder));
        assert_eq!(query.document_no.as_deref(), Some("SO-001"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = BusinessDocumentListParams {
            document_type: None,
            document_no: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn workflow_action_list_params_normalize() {
        let params = WorkflowActionListParams {
            document_id: Some(BusinessDocumentId::new("order-1")),
            actor_id: Some(" user-1 ".to_string()),
            action_type: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.document_id.as_deref(), Some("order-1"));
        assert_eq!(query.actor_id.as_deref(), Some("user-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
    }

    #[test]
    fn relation_request_rejects_self_relation_via_entity() {
        let request: CreateDocumentRelationRequest = serde_json::from_value(json!({
            "from_document_id": "doc-1",
            "to_document_id": "doc-1",
            "relation_type": "CHANGES",
        }))
        .unwrap();
        let data = request.into_data();
        assert!(
            entities::document_registry::DocumentRelation::new(
                entities::ids::DocumentRelationId::new("rel-1"),
                data,
            )
            .is_err(),
            "单据不能与自己建立关系"
        );
        assert_eq!(json!(DocumentRelationType::Changes), json!("CHANGES"));
    }
}
