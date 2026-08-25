//! `document_relation`：原单与变更、退货、退款、冲正、红票的关系（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{BusinessDocumentId, DocumentRelationId};

/// 关系类型（数据模型 §6.1：`CHANGES`、`RETURNS`、`REFUNDS`、`REVERSES`、
/// `RED_OF`、`DERIVED_FROM`；固定枚举，无文档状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DocumentRelationType {
    /// 变更：from 是 to 的变更单。
    Changes,
    /// 退货：from 是 to 的退货单。
    Returns,
    /// 退款：from 是 to 的退款单。
    Refunds,
    /// 冲正：from 是 to 的冲正单。
    Reverses,
    /// 红票：from 是 to 的红字发票。
    RedOf,
    /// 派生：from 由 to 派生（如采购单由销售单派生）。
    DerivedFrom,
}

impl DocumentRelationType {
    /// 返回关系类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Changes => "变更",
            Self::Returns => "退货",
            Self::Refunds => "退款",
            Self::Reverses => "冲正",
            Self::RedOf => "红票",
            Self::DerivedFrom => "派生",
        }
    }

    /// 返回关系类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Changes => "CHANGES",
            Self::Returns => "RETURNS",
            Self::Refunds => "REFUNDS",
            Self::Reverses => "REVERSES",
            Self::RedOf => "RED_OF",
            Self::DerivedFrom => "DERIVED_FROM",
        }
    }
}

/// 单据关系创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRelationData {
    /// 变更、退货、退款、冲正或红票单。
    pub from_document_id: BusinessDocumentId,
    /// 被纠正或被引用的原单。
    pub to_document_id: BusinessDocumentId,
    /// 关系类型。
    pub relation_type: DocumentRelationType,
}

/// 单据关系实体（数据模型 §6.1）。
///
/// `(from_document_id, to_document_id, relation_type)` 唯一约束与
/// `to_document_id + relation_type` 反向查询索引由 P2 建立；关系的方向语义
/// （原单与纠错单双向导航，§8.3 退款/冲正/红票保留原事实）由各域 P3 事务维护。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct DocumentRelation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 变更、退货、退款、冲正或红票单。
    pub from_document_id: BusinessDocumentId,
    /// 被纠正或被引用的原单。
    pub to_document_id: BusinessDocumentId,
    /// 关系类型。
    pub relation_type: DocumentRelationType,
}

impl DocumentRelation {
    /// 创建单据关系。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::DocumentRelationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的关系实体。
    ///
    /// # 错误
    /// 当 `from_document_id` 与 `to_document_id` 相同（单据不能与自己建立
    /// 关系）时返回错误。
    pub fn new(id: DocumentRelationId, data: DocumentRelationData) -> Result<Self> {
        if data.from_document_id == data.to_document_id {
            return Err(Error::from("单据不能与自己建立关系"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            from_document_id: data.from_document_id,
            to_document_id: data.to_document_id,
            relation_type: data.relation_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentRelation, DocumentRelationData, DocumentRelationType};
    use crate::errors::Error;
    use crate::ids::{BusinessDocumentId, DocumentRelationId};

    fn data() -> DocumentRelationData {
        DocumentRelationData {
            from_document_id: BusinessDocumentId::new("change-1"),
            to_document_id: BusinessDocumentId::new("order-1"),
            relation_type: DocumentRelationType::Changes,
        }
    }

    /// happy path：方向与类型正确落库。
    #[test]
    fn new_keeps_direction_and_relation_type() {
        let relation = DocumentRelation::new(DocumentRelationId::new("rel-1"), data()).unwrap();
        assert_eq!(relation.from_document_id, BusinessDocumentId::new("change-1"));
        assert_eq!(relation.to_document_id, BusinessDocumentId::new("order-1"));
        assert_eq!(relation.relation_type, DocumentRelationType::Changes);
    }

    /// 失败路径：同一单据不能与自己建立关系。
    #[test]
    fn new_rejects_self_relation() {
        let payload = DocumentRelationData {
            from_document_id: BusinessDocumentId::new("order-1"),
            ..data()
        };
        let error = DocumentRelation::new(DocumentRelationId::new("rel-1"), payload).unwrap_err();
        match error {
            Error::LogicError(message) => assert!(message.contains("不能与自己")),
            other => panic!("期望 LogicError，得到 {other:?}"),
        }
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn relation_type_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&DocumentRelationType::RedOf).unwrap(),
            "\"RED_OF\""
        );
        assert_eq!(DocumentRelationType::Refunds.as_str(), "REFUNDS");
        assert_eq!(DocumentRelationType::Reverses.label(), "冲正");
        assert_eq!(DocumentRelationType::DerivedFrom.label(), "派生");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let relation = DocumentRelation::new(DocumentRelationId::new("rel-1"), data()).unwrap();
        let roundtrip: DocumentRelation =
            bson::deserialize_from_document(bson::serialize_to_document(&relation).unwrap()).unwrap();
        assert_eq!(roundtrip, relation);
    }
}
