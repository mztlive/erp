//! `document_attachment`：文件资产与业务单据的关联（数据模型 §6.1 表目录）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use crate::validation::normalize_required_text;

/// 创建人标识最大长度。
const CREATED_BY_MAX_LEN: usize = 128;

/// 附件用途（数据模型 §6.1 / §4.4：附件、图片、清单等；固定枚举）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentUsage {
    /// 附件。
    Attachment,
    /// 图片。
    Image,
    /// 清单（manifest 等结构化清单文件）。
    Manifest,
}

impl AttachmentUsage {
    /// 返回用途的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Attachment => "附件",
            Self::Image => "图片",
            Self::Manifest => "清单",
        }
    }

    /// 返回用途的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Attachment => "attachment",
            Self::Image => "image",
            Self::Manifest => "manifest",
        }
    }
}

/// 附件关联创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentAttachmentData {
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: BusinessDocumentId,
    /// 受控文件资产。
    pub file_asset_id: FileAssetId,
    /// 用途。
    pub usage: AttachmentUsage,
    /// 记录人。
    pub created_by: String,
}

/// 业务单据附件关联实体（数据模型 §6.1）。
///
/// 文件资产存在即可建立关联；安全检查、保留期与销毁状态只作治理记录。
/// 关联本身只追加不删除（§4.5.7 审计留痕）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct DocumentAttachment {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 业务单据。
    pub document_id: BusinessDocumentId,
    /// 受控文件资产。
    pub file_asset_id: FileAssetId,
    /// 用途。
    pub usage: AttachmentUsage,
    /// 记录人。
    pub created_by: String,
}

impl DocumentAttachment {
    /// 创建附件关联。
    ///
    /// 完成 created_by 的校验与规范化（trim、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::DocumentAttachmentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的附件关联。
    ///
    /// # 错误
    /// 当记录人为空或超长时返回错误。
    pub fn new(id: DocumentAttachmentId, data: DocumentAttachmentData) -> Result<Self> {
        let created_by = normalize_required_text(
            data.created_by,
            "记录人不能为空",
            CREATED_BY_MAX_LEN,
            "记录人过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            document_id: data.document_id,
            file_asset_id: data.file_asset_id,
            usage: data.usage,
            created_by,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AttachmentUsage, DocumentAttachment, DocumentAttachmentData};
    use crate::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};

    fn data() -> DocumentAttachmentData {
        DocumentAttachmentData {
            document_id: BusinessDocumentId::new("order-1"),
            file_asset_id: FileAssetId::new("file-1"),
            usage: AttachmentUsage::Attachment,
            created_by: " admin-1 ".to_string(),
        }
    }

    /// happy path：记录人 trim，关联 ID 与用途落库。
    #[test]
    fn new_trims_creator_and_keeps_links() {
        let attachment = DocumentAttachment::new(DocumentAttachmentId::new("da-1"), data()).unwrap();
        assert_eq!(attachment.created_by, "admin-1");
        assert_eq!(attachment.document_id, BusinessDocumentId::new("order-1"));
        assert_eq!(attachment.file_asset_id, FileAssetId::new("file-1"));
        assert_eq!(attachment.usage, AttachmentUsage::Attachment);
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_creator() {
        let payload = DocumentAttachmentData {
            created_by: "  ".to_string(),
            ..data()
        };
        assert!(DocumentAttachment::new(DocumentAttachmentId::new("da-1"), payload).is_err());
    }

    /// 失败路径：超长记录人被拒。
    #[test]
    fn new_rejects_overlong_creator() {
        let payload = DocumentAttachmentData {
            created_by: "x".repeat(129),
            ..data()
        };
        assert!(DocumentAttachment::new(DocumentAttachmentId::new("da-1"), payload).is_err());
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn usage_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&AttachmentUsage::Manifest).unwrap(),
            "\"manifest\""
        );
        assert_eq!(AttachmentUsage::Image.as_str(), "image");
        assert_eq!(AttachmentUsage::Attachment.label(), "附件");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let attachment = DocumentAttachment::new(DocumentAttachmentId::new("da-1"), data()).unwrap();
        let roundtrip: DocumentAttachment =
            bson::deserialize_from_document(bson::serialize_to_document(&attachment).unwrap()).unwrap();
        assert_eq!(roundtrip, attachment);
    }
}
