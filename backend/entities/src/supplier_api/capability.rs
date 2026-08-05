//! `supplier_api_capability`：连接能力声明（数据模型 §6.14，页面 W20）。
//!
//! 能力声明只表示供应商接口具备该操作，不表示每个商品都必然可用（phase-2 §6.1）；
//! 商品级能力由发布修订的 `product_capabilities` 另行保存。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
use crate::validation::normalize_optional_text;

/// 能力约束快照最大长度。
const CONSTRAINT_SNAPSHOT_MAX_LEN: usize = 2000;

/// 能力代码（数据模型 §6.14：商品、价格、库存、下单、查询、取消、退款、物流、
/// 回调、结算等；固定枚举，禁止运行时扩展）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierApiCapabilityCode {
    /// 商品同步。
    Product,
    /// 价格同步。
    Price,
    /// 库存同步。
    Stock,
    /// 下单。
    Order,
    /// 查询。
    Query,
    /// 取消。
    Cancel,
    /// 退款。
    Refund,
    /// 物流查询。
    Logistics,
    /// 状态回调。
    Callback,
    /// 结算同步。
    Settlement,
}

impl SupplierApiCapabilityCode {
    /// 返回能力代码的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Product => "商品",
            Self::Price => "价格",
            Self::Stock => "库存",
            Self::Order => "下单",
            Self::Query => "查询",
            Self::Cancel => "取消",
            Self::Refund => "退款",
            Self::Logistics => "物流",
            Self::Callback => "回调",
            Self::Settlement => "结算",
        }
    }

    /// 返回能力代码的稳定字符串。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Price => "price",
            Self::Stock => "stock",
            Self::Order => "order",
            Self::Query => "query",
            Self::Cancel => "cancel",
            Self::Refund => "refund",
            Self::Logistics => "logistics",
            Self::Callback => "callback",
            Self::Settlement => "settlement",
        }
    }
}

/// 能力启停状态（数据模型 §6.14：启用/停用；固定枚举，无文档状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierApiCapabilityStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl SupplierApiCapabilityStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// 判断能力是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 连接能力创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierApiCapabilityData {
    /// 所属连接。
    pub connection_id: SupplierApiConnectionId,
    /// 能力代码。
    pub capability_code: SupplierApiCapabilityCode,
    /// 能力启停状态。
    pub status: SupplierApiCapabilityStatus,
    /// 供应商能力限制快照。
    pub constraint_snapshot: Option<String>,
}

/// 连接能力更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierApiCapabilityUpdate {
    /// 能力启停状态；`None` 表示不修改。
    pub status: Option<SupplierApiCapabilityStatus>,
    /// 供应商能力限制快照；`None` 表示不修改。
    pub constraint_snapshot: Option<String>,
}

/// 连接能力声明实体（注册表行，数据模型 §6.14）。
///
/// 不属稳定基础资料或正式事实，只用 `BaseModel` 承载持久化元数据（判定同
/// `source_registry.external_identity_map`）；`(connection_id, capability_code)`
/// 唯一约束由唯一索引保证（§6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierApiCapability {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属连接。
    pub connection_id: SupplierApiConnectionId,
    /// 能力代码（创建后不可修改）。
    pub capability_code: SupplierApiCapabilityCode,
    /// 能力启停状态。
    pub status: SupplierApiCapabilityStatus,
    /// 供应商能力限制快照。
    pub constraint_snapshot: Option<String>,
}

impl SupplierApiCapability {
    /// 创建连接能力声明。
    ///
    /// 完成 constraint_snapshot 的校验与规范化（去首尾空白、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierApiCapabilityId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的能力声明实体。
    ///
    /// # 错误
    /// 当能力约束快照超长时返回错误。
    pub fn new(id: SupplierApiCapabilityId, data: SupplierApiCapabilityData) -> Result<Self> {
        let constraint_snapshot = normalize_optional_text(
            data.constraint_snapshot,
            "能力约束快照",
            CONSTRAINT_SNAPSHOT_MAX_LEN,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            connection_id: data.connection_id,
            capability_code: data.capability_code,
            status: data.status,
            constraint_snapshot,
        })
    }

    /// 更新连接能力声明。
    ///
    /// 复用 `new` 的校验规则；`connection_id`/`capability_code` 是稳定键，
    /// 不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当能力约束快照超长时返回错误。
    pub fn update(&mut self, update: SupplierApiCapabilityUpdate) -> Result<()> {
        self.apply_status(update.status);
        self.apply_constraint_snapshot(update.constraint_snapshot)
    }

    /// 判断能力是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// 应用启停状态更新。
    ///
    /// # 参数
    /// * `status` - 可选启停状态
    fn apply_status(&mut self, status: Option<SupplierApiCapabilityStatus>) {
        if let Some(status) = status {
            self.status = status;
        }
    }

    /// 应用能力约束快照更新。
    ///
    /// # 参数
    /// * `constraint_snapshot` - 可选约束快照
    ///
    /// # 错误
    /// 当约束快照超长时返回错误。
    fn apply_constraint_snapshot(&mut self, constraint_snapshot: Option<String>) -> Result<()> {
        if let Some(constraint_snapshot) = constraint_snapshot {
            self.constraint_snapshot = normalize_optional_text(
                Some(constraint_snapshot),
                "能力约束快照",
                CONSTRAINT_SNAPSHOT_MAX_LEN,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiCapabilityData,
        SupplierApiCapabilityStatus, SupplierApiCapabilityUpdate,
    };
    use crate::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};

    fn capability_data() -> SupplierApiCapabilityData {
        SupplierApiCapabilityData {
            connection_id: SupplierApiConnectionId::new("conn-1"),
            capability_code: SupplierApiCapabilityCode::Order,
            status: SupplierApiCapabilityStatus::Active,
            constraint_snapshot: Some(" 单笔下单金额上限 50000 元 ".to_string()),
        }
    }

    #[test]
    fn capability_new_trims_constraint_snapshot() {
        let capability =
            SupplierApiCapability::new(SupplierApiCapabilityId::new("cap-1"), capability_data()).unwrap();

        assert_eq!(capability.connection_id, SupplierApiConnectionId::new("conn-1"));
        assert_eq!(capability.capability_code, SupplierApiCapabilityCode::Order);
        assert_eq!(
            capability.constraint_snapshot.as_deref(),
            Some("单笔下单金额上限 50000 元")
        );
        assert!(capability.is_active());
    }

    #[test]
    fn capability_new_rejects_overlong_constraint_snapshot() {
        let overlong = SupplierApiCapabilityData {
            constraint_snapshot: Some("c".repeat(2001)),
            ..capability_data()
        };
        assert!(SupplierApiCapability::new(SupplierApiCapabilityId::new("cap-2"), overlong).is_err());
    }

    #[test]
    fn capability_new_accepts_missing_constraint_snapshot() {
        let without_snapshot = SupplierApiCapabilityData {
            constraint_snapshot: None,
            ..capability_data()
        };
        let capability =
            SupplierApiCapability::new(SupplierApiCapabilityId::new("cap-3"), without_snapshot).unwrap();
        assert!(capability.constraint_snapshot.is_none());
    }

    #[test]
    fn capability_update_changes_status_and_snapshot_but_keeps_codes() {
        let mut capability =
            SupplierApiCapability::new(SupplierApiCapabilityId::new("cap-1"), capability_data()).unwrap();

        capability
            .update(SupplierApiCapabilityUpdate {
                status: Some(SupplierApiCapabilityStatus::Disabled),
                constraint_snapshot: Some(" 停用期间的额外限制 ".to_string()),
            })
            .unwrap();

        assert!(!capability.is_active());
        assert_eq!(
            capability.constraint_snapshot.as_deref(),
            Some("停用期间的额外限制")
        );
        assert_eq!(capability.capability_code, SupplierApiCapabilityCode::Order);
    }

    #[test]
    fn capability_update_rejects_overlong_constraint_snapshot() {
        let mut capability =
            SupplierApiCapability::new(SupplierApiCapabilityId::new("cap-1"), capability_data()).unwrap();

        assert!(capability
            .update(SupplierApiCapabilityUpdate {
                status: None,
                constraint_snapshot: Some("c".repeat(2001)),
            })
            .is_err());
    }

    #[test]
    fn capability_enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&SupplierApiCapabilityCode::Settlement).unwrap(),
            "\"settlement\""
        );
        assert_eq!(
            serde_json::to_string(&SupplierApiCapabilityStatus::Disabled).unwrap(),
            "\"disabled\""
        );
        assert_eq!(SupplierApiCapabilityCode::Logistics.label(), "物流");
        assert_eq!(SupplierApiCapabilityCode::Callback.as_str(), "callback");
        assert_eq!(SupplierApiCapabilityStatus::Active.label(), "启用");
    }

    #[test]
    fn capability_rejects_unknown_code_on_deserialize() {
        assert!(serde_json::from_str::<SupplierApiCapabilityCode>("\"unknown_code\"").is_err());
    }
}
