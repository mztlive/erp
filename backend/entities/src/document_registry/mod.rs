//! 域 D02 `document_registry`：business_document、document_relation、document_participant、workflow_action（页面：全部单据页）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与必需约束见数据模型 §6.1；公共字段归属按 §4.3 判定：
//! - 本域四张表都是「跨域单据稳定注册表 / 关系 / 历史参与人 / 追加式动作」，
//!   不属稳定基础资料、不可变修订或正式事实 → 只用 `BaseModel` 持久化元数据，
//!   业务字段按 §6.1 各自建模，不硬套 StableBase 的 status/created_by 语义；
//! - `business_document` 是跨域关联注册表，不是万能单据表（§5.1），
//!   强类型业务表一对一注册，不允许脱离业务表单独创建空单据（§6.1，P3 校验）；
//! - `workflow_action.from_status / to_status` 记录的是**其它域单据**的状态迁移
//!   代码（跨域开放目录，无法在单域内固化为枚举），按稳定代码字符串建模
//!   （校验大写代码形态），跨域一致性核对留给 P5（数据模型第 7 章）。

pub mod business_document;
pub mod document_participant;
pub mod document_relation;
pub mod workflow_action;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{BusinessDocumentId, DocumentParticipantId, DocumentRelationId, WorkflowActionId};
pub use business_document::{BusinessDocument, BusinessDocumentData, DocumentType};
pub use document_participant::{DocumentParticipant, DocumentParticipantData, ParticipantRole};
pub use document_relation::{DocumentRelation, DocumentRelationData, DocumentRelationType};
pub use workflow_action::{WorkflowAction, WorkflowActionData, WorkflowActionType};
