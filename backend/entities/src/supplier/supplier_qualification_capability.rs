//! `supplier_qualification_capability`：资质适用能力（数据模型 §6.2）。
//!
//! 明确资质适用的供应商能力；启用可销售公司 SKU、采购单和供给关系时
//! 必须校验适用能力存在有效资质（跨聚合约束，P3 事务校验，§6.2 必需
//! 约束，条目 P3-§6.2-qualification-gate）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;

pub use crate::ids::{SupplierCapabilityId, SupplierQualificationCapabilityId, SupplierQualificationId};

/// 资质适用能力创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierQualificationCapabilityData {
    /// 资质 ID。
    pub qualification_id: SupplierQualificationId,
    /// 适用能力 ID。
    pub capability_id: SupplierCapabilityId,
}

/// 资质适用能力实体（纯关联行，§6.2：`supplier_qualification_capability`
/// 明确适用能力）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierQualificationCapability {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 资质 ID。
    pub qualification_id: SupplierQualificationId,
    /// 适用能力 ID。
    pub capability_id: SupplierCapabilityId,
}

impl SupplierQualificationCapability {
    /// 创建资质适用能力关联。
    ///
    /// 关联行按「资质与其适用能力」的成对关系整体替换维护（P3 事务
    /// 重写关联集合），不做原地修改。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierQualificationCapabilityId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的关联实体。
    pub fn new(
        id: SupplierQualificationCapabilityId,
        data: SupplierQualificationCapabilityData,
    ) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            qualification_id: data.qualification_id,
            capability_id: data.capability_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{SupplierQualificationCapability, SupplierQualificationCapabilityData};
    use crate::ids::{SupplierCapabilityId, SupplierQualificationCapabilityId, SupplierQualificationId};

    /// happy path：成对关联落库。
    #[test]
    fn new_links_qualification_to_capability() {
        let link = SupplierQualificationCapability::new(
            SupplierQualificationCapabilityId::new("link-1"),
            SupplierQualificationCapabilityData {
                qualification_id: SupplierQualificationId::new("qual-1"),
                capability_id: SupplierCapabilityId::new("cap-1"),
            },
        )
        .unwrap();
        assert_eq!(link.qualification_id, SupplierQualificationId::new("qual-1"));
        assert_eq!(link.capability_id, SupplierCapabilityId::new("cap-1"));
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let link = SupplierQualificationCapability::new(
            SupplierQualificationCapabilityId::new("link-2"),
            SupplierQualificationCapabilityData {
                qualification_id: SupplierQualificationId::new("qual-2"),
                capability_id: SupplierCapabilityId::new("cap-2"),
            },
        )
        .unwrap();
        let roundtrip: SupplierQualificationCapability =
            bson::deserialize_from_document(bson::serialize_to_document(&link).unwrap()).unwrap();
        assert_eq!(roundtrip, link);
    }
}
