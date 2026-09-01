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

    /// 为指定资质批量构造完整适用能力关联集合。
    ///
    /// # 参数
    /// * `qualification_id` - 资质稳定主键
    /// * `capability_codes` - 请求携带的适用能力代码集合，按输入顺序保留；由调用方保证已去重并与 `capability_ids` 交集校验
    /// * `capability_ids` - 当前有效能力代码到稳定能力 ID 的映射，必须包含 `capability_codes` 的全部代码
    /// * `link_ids` - 关联主键集合，按 `capability_codes` 顺序一一对应，由 Service 分配
    ///
    /// # 返回
    /// 返回与当前能力集合一一对应的关联实体；若 `capability_codes` 为空则返回空集合。
    ///
    /// # 错误
    /// `capability_codes` 中任一代码未在 `capability_ids` 中、或 `link_ids` 长度不一致时返回错误。
    ///
    /// # 约束
    /// 纯内存，不触及 MongoDB 或 ID 生成器；关联必须无遗漏、无重复且只引用当前能力，重复由调用方通过 `validate_profile_selection` 前置去重。
    pub fn links_for_qualification(
        qualification_id: SupplierQualificationId,
        capability_codes: &[crate::supplier::CapabilityCode],
        capability_ids: &std::collections::HashMap<String, SupplierCapabilityId>,
        link_ids: Vec<SupplierQualificationCapabilityId>,
    ) -> crate::Result<Vec<Self>> {
        if capability_codes.len() != link_ids.len() {
            return Err(crate::Error::from("资质适用能力与关联 ID 数量不一致"));
        }
        let mut links = Vec::with_capacity(capability_codes.len());
        for (code, link_id) in capability_codes.iter().zip(link_ids) {
            let capability_id = capability_ids
                .get(code.as_str())
                .ok_or_else(|| crate::Error::from("资质适用能力不存在"))?;
            links.push(Self::new(
                link_id,
                SupplierQualificationCapabilityData {
                    qualification_id: qualification_id.clone(),
                    capability_id: capability_id.clone(),
                },
            )?);
        }
        Ok(links)
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

    /// 关联按当前能力集合完整重建，空集合与缺失均覆盖。
    #[test]
    fn links_for_qualification_covers_empty_and_missing() {
        use crate::supplier::CapabilityCode;
        use std::collections::HashMap;
        let mut ids = HashMap::new();
        ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-1"));
        // 空集合返回空
        let empty = SupplierQualificationCapability::links_for_qualification(
            SupplierQualificationId::new("qual-1"),
            &[],
            &ids,
            vec![],
        )
        .unwrap();
        assert!(empty.is_empty());
        // 缺失能力报错
        let err = SupplierQualificationCapability::links_for_qualification(
            SupplierQualificationId::new("qual-2"),
            &[CapabilityCode::Api],
            &ids,
            vec![SupplierQualificationCapabilityId::new("link-1")],
        )
        .unwrap_err();
        assert!(err.to_string().contains("资质适用能力不存在"));
        // 长度不一致报错
        let err2 = SupplierQualificationCapability::links_for_qualification(
            SupplierQualificationId::new("qual-3"),
            &[CapabilityCode::Physical],
            &ids,
            vec![],
        )
        .unwrap_err();
        assert!(err2.to_string().contains("数量不一致"));
        // 正确映射且只引用当前能力
        let links = SupplierQualificationCapability::links_for_qualification(
            SupplierQualificationId::new("qual-4"),
            &[CapabilityCode::Physical],
            &ids,
            vec![SupplierQualificationCapabilityId::new("link-4")],
        )
        .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].capability_id, SupplierCapabilityId::new("cap-1"));
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
