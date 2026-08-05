//! `document_participant`：单据历史参与人及查看依据（数据模型 §6.1 / §4.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::ids::{BusinessDocumentId, DocumentParticipantId};
use crate::validation::normalize_required_text;

/// 用户 ID 最大长度。
const USER_ID_MAX_LEN: usize = 128;
/// 参与人名称快照最大长度。
const NAME_MAX_LEN: usize = 128;
/// 记录人标识最大长度。
const RECORDED_BY_MAX_LEN: usize = 128;

/// 参与角色（数据模型 §6.1 / erp-phase-1 §4.4：当时负责人和协作销售；
/// 固定枚举，其余角色属二期扩展的地基修订候选）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    /// 主负责销售（当时快照）。
    OwnerSales,
    /// 协作销售（当时快照）。
    CoSales,
}

impl ParticipantRole {
    /// 返回角色类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::OwnerSales => "主负责销售",
            Self::CoSales => "协作销售",
        }
    }

    /// 返回角色类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OwnerSales => "owner_sales",
            Self::CoSales => "co_sales",
        }
    }
}

/// 单据参与人创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentParticipantData {
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: BusinessDocumentId,
    /// 参与角色。
    pub participant_role: ParticipantRole,
    /// 参与人用户 ID。
    pub participant_user_id: String,
    /// 参与人名称快照（参与当时显示名，避免后续改名反推历史）。
    pub participant_name: String,
    /// 记录人（账号或系统身份）。
    pub recorded_by: String,
}

/// 单据历史参与人实体（数据模型 §6.1）。
///
/// 客户历史参与者查看权写入本表，不依赖当前客户负责人反推（§4.6）；
/// 负责人变更后原负责人不再查看该客户的新增单据，已参与的历史单据保留
/// 查看权（erp-phase-1 §11.1），参与记录只追加不删除。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct DocumentParticipant {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: BusinessDocumentId,
    /// 参与角色。
    pub participant_role: ParticipantRole,
    /// 参与人用户 ID。
    pub participant_user_id: String,
    /// 参与人名称快照。
    pub participant_name: String,
    /// 记录人。
    pub recorded_by: String,
}

impl DocumentParticipant {
    /// 创建单据参与人记录。
    ///
    /// 完成全部文本字段的校验与规范化（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::DocumentParticipantId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的参与人记录。
    ///
    /// # 错误
    /// 当参与人用户 ID、名称或记录人为空/超长时返回错误。
    pub fn new(id: DocumentParticipantId, data: DocumentParticipantData) -> Result<Self> {
        let participant_user_id = normalize_required_text(
            data.participant_user_id,
            "参与人用户ID不能为空",
            USER_ID_MAX_LEN,
            "参与人用户ID过长",
        )?;
        let participant_name = normalize_required_text(
            data.participant_name,
            "参与人名称不能为空",
            NAME_MAX_LEN,
            "参与人名称过长",
        )?;
        let recorded_by = normalize_required_text(
            data.recorded_by,
            "记录人不能为空",
            RECORDED_BY_MAX_LEN,
            "记录人过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            document_id: data.document_id,
            participant_role: data.participant_role,
            participant_user_id,
            participant_name,
            recorded_by,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentParticipant, DocumentParticipantData, ParticipantRole};
    use crate::ids::{BusinessDocumentId, DocumentParticipantId};

    fn data() -> DocumentParticipantData {
        DocumentParticipantData {
            document_id: BusinessDocumentId::new("order-1"),
            participant_role: ParticipantRole::OwnerSales,
            participant_user_id: " user-1 ".to_string(),
            participant_name: " 张三 ".to_string(),
            recorded_by: " admin-1 ".to_string(),
        }
    }

    /// happy path：全部文本字段去首尾空白。
    #[test]
    fn new_trims_all_text_fields() {
        let participant = DocumentParticipant::new(DocumentParticipantId::new("dp-1"), data()).unwrap();
        assert_eq!(participant.participant_user_id, "user-1");
        assert_eq!(participant.participant_name, "张三");
        assert_eq!(participant.recorded_by, "admin-1");
        assert_eq!(participant.participant_role, ParticipantRole::OwnerSales);
        assert_eq!(participant.document_id, BusinessDocumentId::new("order-1"));
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_user_id() {
        let payload = DocumentParticipantData {
            participant_user_id: "   ".to_string(),
            ..data()
        };
        assert!(DocumentParticipant::new(DocumentParticipantId::new("dp-1"), payload).is_err());
    }

    /// 失败路径：超长名称被拒。
    #[test]
    fn new_rejects_overlong_name() {
        let payload = DocumentParticipantData {
            participant_name: "名".repeat(129),
            ..data()
        };
        assert!(DocumentParticipant::new(DocumentParticipantId::new("dp-1"), payload).is_err());
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn participant_role_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&ParticipantRole::CoSales).unwrap(),
            "\"co_sales\""
        );
        assert_eq!(ParticipantRole::OwnerSales.as_str(), "owner_sales");
        assert_eq!(ParticipantRole::OwnerSales.label(), "主负责销售");
        assert_eq!(ParticipantRole::CoSales.label(), "协作销售");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let participant = DocumentParticipant::new(DocumentParticipantId::new("dp-1"), data()).unwrap();
        let roundtrip: DocumentParticipant =
            bson::from_document(bson::to_document(&participant).unwrap()).unwrap();
        assert_eq!(roundtrip, participant);
    }
}
