//! `sales_order_projection`：销售单执行投影稳定身份（数据模型 §6.16，页面 W23）。
//!
//! 投影是「ERP 销售单 → 目标商城」的执行指令稳定身份（phase-2 §14.2），
//! 字典无状态与审计字段 → 只用 `BaseModel` 承载持久化元数据（判定同
//! `source_registry.external_identity_map`）；`sales_order_id`/`target_mall_id`
//! 为稳定键，`(sales_order_id, target_mall_id)` 唯一约束由唯一索引保证（§6.16）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::ids::{SalesOrderId, SalesOrderProjectionId, SalesOrderProjectionRevisionId, SourceSystemId};

/// 销售单执行投影创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionData {
    /// 卡券销售单。
    pub sales_order_id: SalesOrderId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
}

/// 销售单执行投影更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesOrderProjectionUpdate {
    /// 商城最后确认版本；`None` 表示不修改。
    pub current_acked_revision_id: Option<SalesOrderProjectionRevisionId>,
}

/// 销售单执行投影实体（数据模型 §6.16）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderProjection {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 卡券销售单。
    pub sales_order_id: SalesOrderId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 商城最后确认版本。
    pub current_acked_revision_id: Option<SalesOrderProjectionRevisionId>,
}

impl SalesOrderProjection {
    /// 创建销售单执行投影。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderProjectionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的投影实体（商城最后确认版本为空）。
    pub fn new(id: SalesOrderProjectionId, data: SalesOrderProjectionData) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_order_id: data.sales_order_id,
            target_mall_id: data.target_mall_id,
            current_acked_revision_id: None,
        })
    }

    /// 更新销售单执行投影。
    ///
    /// `sales_order_id`/`target_mall_id` 是稳定键（§6.16 `(sales_order_id,
    /// target_mall_id)` 唯一），不允许在通用更新中修改；只推进商城最后确认版本。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, update: SalesOrderProjectionUpdate) -> Result<()> {
        self.apply_acked_revision(update.current_acked_revision_id);
        Ok(())
    }

    /// 应用商城最后确认版本更新。
    ///
    /// # 参数
    /// * `current_acked_revision_id` - 可选确认版本
    fn apply_acked_revision(&mut self, current_acked_revision_id: Option<SalesOrderProjectionRevisionId>) {
        if let Some(current_acked_revision_id) = current_acked_revision_id {
            self.current_acked_revision_id = Some(current_acked_revision_id);
        }
    }

    /// 判断商城确认指针是否应推进到新版本。
    ///
    /// 用于 W23 投递成功后决定是否将投影的 `current_acked_revision_id` 前进。
    /// 指针只允许单调不减，已确认的旧版本回退视为无效。
    ///
    /// # 参数
    /// * `current_revision_no` - 当前已确认修订序号
    /// * `incoming_revision_no` - 新确认修订序号
    ///
    /// # 返回
    /// 新版本序号大于等于当前序号时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 该方法为纯序号比较，不触及持久化或外部状态；同修订重复 ACK 场景应由
    /// 调用方先比较修订身份，单调性仅用于序号不同的推进判断。
    pub fn should_advance_acked_revision(current_revision_no: u32, incoming_revision_no: u32) -> bool {
        incoming_revision_no >= current_revision_no
    }

    /// 判断给定修订是否为当前已确认修订的重复确认。
    ///
    /// 用于区分同修订重复 ACK 与同序号不同身份的场景。
    ///
    /// # 参数
    /// * `incoming_revision_id` - 新确认修订身份
    ///
    /// # 返回
    /// 与当前已确认身份相同时返回 `true`，表示重复 ACK 不应重复推进。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 该方法仅比较身份，不触及持久化；调用方应在加载当前修订后调用。
    pub fn is_same_acked_revision(&self, incoming_revision_id: &SalesOrderProjectionRevisionId) -> bool {
        self.current_acked_revision_id
            .as_ref()
            .is_some_and(|current| current == incoming_revision_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{SalesOrderProjection, SalesOrderProjectionData, SalesOrderProjectionUpdate};
    use crate::ids::{SalesOrderId, SalesOrderProjectionId, SalesOrderProjectionRevisionId, SourceSystemId};

    fn projection_data() -> SalesOrderProjectionData {
        SalesOrderProjectionData {
            sales_order_id: SalesOrderId::new("so-1"),
            target_mall_id: SourceSystemId::new("mall-1"),
        }
    }

    #[test]
    fn projection_new_builds_without_acked_revision() {
        let projection =
            SalesOrderProjection::new(SalesOrderProjectionId::new("proj-1"), projection_data()).unwrap();

        assert_eq!(projection.sales_order_id, SalesOrderId::new("so-1"));
        assert_eq!(projection.target_mall_id, SourceSystemId::new("mall-1"));
        assert!(projection.current_acked_revision_id.is_none());
    }

    #[test]
    fn projection_update_advances_acked_revision_and_keeps_stable_keys() {
        let mut projection =
            SalesOrderProjection::new(SalesOrderProjectionId::new("proj-1"), projection_data()).unwrap();

        projection
            .update(SalesOrderProjectionUpdate {
                current_acked_revision_id: Some(SalesOrderProjectionRevisionId::new("proj-rev-1")),
            })
            .unwrap();

        assert_eq!(
            projection.current_acked_revision_id,
            Some(SalesOrderProjectionRevisionId::new("proj-rev-1"))
        );
        assert_eq!(
            projection.sales_order_id,
            SalesOrderId::new("so-1"),
            "稳定键不可修改"
        );
        assert_eq!(projection.target_mall_id, SourceSystemId::new("mall-1"));
    }
}
