//! `warehouse` 自有仓库稳定身份（数据模型 §6.3，稳定主表）。
//!
//! `warehouse_code` 唯一（唯一约束跨行，属 P3/索引校验）；
//! 「有库存或有效预占时不得停用」需要库存域数据，属跨聚合校验，留 P3。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::stable::StableBase;
use erp_core::common::state::ensure_transition;
use erp_core::ids::WarehouseId;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::status::EnableStatus;
use super::warehouse_revision::WarehouseRevision;

/// 仓库代码最大长度。
const WAREHOUSE_CODE_MAX_LEN: usize = 64;
/// 仓储经办人账号 ID 最大长度。
const HANDLER_USER_ID_MAX_LEN: usize = 128;

/// 仓库履约责任类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WarehouseFulfillmentOperation {
    /// 采购到货入库。
    Receipt,
    /// 公司仓发货。
    WarehouseShip,
}

/// 仓库创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseData {
    /// ERP 仓库稳定代码（唯一，创建后不可修改）。
    pub warehouse_code: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 入库经办人；旧数据允许为空，生成任务时失败关闭。
    pub inbound_handler_user_id: Option<String>,
    /// 仓发经办人；旧数据允许为空，生成任务时失败关闭。
    pub outbound_handler_user_id: Option<String>,
}

impl WarehouseData {
    /// 从 ERP 仓库稳定代码准备创建数据，默认启用且不配置经办人；代码唯一性在持久化时约束。
    pub fn new(warehouse_code: impl Into<String>) -> Self {
        Self {
            warehouse_code: warehouse_code.into(),
            status: EnableStatus::Active,
            inbound_handler_user_id: None,
            outbound_handler_user_id: None,
        }
    }

    /// 在构造阶段指定启停状态，用于覆盖创建数据的默认启用值。
    pub fn with_status(mut self, status: EnableStatus) -> Self {
        self.status = status;
        self
    }

    /// 在待创建数据中指定入库经办人账号，供实体校验后保存独立的入库职责。
    pub fn with_inbound_handler_user_id(mut self, inbound_handler_user_id: impl Into<String>) -> Self {
        self.inbound_handler_user_id = Some(inbound_handler_user_id.into());
        self
    }

    /// 在待创建数据中指定仓发经办人账号，避免与入库职责混用。
    pub fn with_outbound_handler_user_id(mut self, outbound_handler_user_id: impl Into<String>) -> Self {
        self.outbound_handler_user_id = Some(outbound_handler_user_id.into());
        self
    }
}

/// 仓库更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WarehouseUpdate {
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
    /// 入库经办人；外层 `None` 表示不修改。
    pub inbound_handler_user_id: Option<Option<String>>,
    /// 仓发经办人；外层 `None` 表示不修改。
    pub outbound_handler_user_id: Option<Option<String>>,
}

/// 仓库实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Warehouse {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// ERP 仓库稳定代码（创建后不可修改）。
    pub warehouse_code: String,
    /// 入库经办人；兼容存量仓库缺少该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbound_handler_user_id: Option<String>,
    /// 仓发经办人；兼容存量仓库缺少该字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_handler_user_id: Option<String>,
}

impl PartialEq for Warehouse {
    // 比较仓库全部字段，当前修订、经办配置或审计元数据不同都不视为同一份记录。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.warehouse_code == other.warehouse_code
            && self.inbound_handler_user_id == other.inbound_handler_user_id
            && self.outbound_handler_user_id == other.outbound_handler_user_id
    }
}

impl Eq for Warehouse {}

impl Warehouse {
    /// 建立稳定仓库身份并记录初始状态、创建人和经办配置。仓库代码去首尾空白后不能为空或超长；经办账号超长报错，空白视为未配置。
    pub fn new(id: WarehouseId, data: WarehouseData, created_by: impl Into<String>) -> Result<Self> {
        let warehouse_code = normalize_required_text(
            data.warehouse_code,
            "仓库代码不能为空",
            WAREHOUSE_CODE_MAX_LEN,
            "仓库代码过长",
        )?;
        let inbound_handler_user_id = normalize_handler_user_id(data.inbound_handler_user_id)?;
        let outbound_handler_user_id = normalize_handler_user_id(data.outbound_handler_user_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            warehouse_code,
            inbound_handler_user_id,
            outbound_handler_user_id,
        })
    }

    /// 更新启停状态和经办配置并记录修改人，不允许改稳定仓库代码；非法状态迁移或经办账号超长时返回错误。有库存或有效预占不得停用是跨域约束，须由用例层核查。
    pub fn update(&mut self, update: WarehouseUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(status) = update.status {
            ensure_transition(self.stable.status, status)?;
            self.stable.status = status;
        }

        if let Some(handler) = update.inbound_handler_user_id {
            self.inbound_handler_user_id = normalize_handler_user_id(handler)?;
        }
        if let Some(handler) = update.outbound_handler_user_id {
            self.outbound_handler_user_id = normalize_handler_user_id(handler)?;
        }

        self.stable.touch(updated_by);
        Ok(())
    }

    /// 只将属于当前仓库的修订 ID 设为当前指针；其它仓库的修订返回错误，不改指针。
    pub fn apply_revision(&mut self, revision: &WarehouseRevision) -> Result<()> {
        if revision.warehouse_id.as_ref() != self.base.id.as_str() {
            return Err(Error::from("仓库修订不属于当前仓库"));
        }
        self.stable.current_revision_id = Some(revision.base.id.clone());
        Ok(())
    }

    /// 对照调用方读取时的期望版本，为修改前的乐观锁判断提供依据。
    pub fn matches_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 根据仓库稳定资料的启停状态判断能否使用，只有 `Active` 才返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 按入库或仓发职责解析启用仓库上配置的唯一经办人。仓库停用或对应人员缺失时拒绝，调用方不得回退给创建人或责任池。
    pub fn fulfillment_handler(&self, operation: WarehouseFulfillmentOperation) -> Result<&str> {
        if !self.is_active() {
            return Err(Error::from("目标仓库已停用，请先更换仓库"));
        }

        let handler = match operation {
            WarehouseFulfillmentOperation::Receipt => self.inbound_handler_user_id.as_deref(),
            WarehouseFulfillmentOperation::WarehouseShip => self.outbound_handler_user_id.as_deref(),
        };
        handler.ok_or_else(|| {
            Error::from(match operation {
                WarehouseFulfillmentOperation::Receipt => "目标仓库未配置入库经办人，请先完成仓库责任配置",
                WarehouseFulfillmentOperation::WarehouseShip => {
                    "目标仓库未配置仓发经办人，请先完成仓库责任配置"
                },
            })
        })
    }
}

// 清理可选经办账号，空白视为未配置，超长返回错误，供创建和更新共用。
fn normalize_handler_user_id(value: Option<String>) -> Result<Option<String>> {
    normalize_optional_text(value, "仓储经办人", HANDLER_USER_ID_MAX_LEN)
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::{assert_adjacency_closed, ensure_transition};
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{WarehouseId, WarehouseRevisionId};

    use super::super::warehouse_revision::{SensitiveText, WarehouseRevisionData};
    use super::*;

    fn data() -> WarehouseData {
        WarehouseData::new(" WH-BJ-001 ")
            .with_inbound_handler_user_id(" inbound-1 ")
            .with_outbound_handler_user_id(" outbound-1 ")
    }

    fn revision(warehouse_id: &str) -> WarehouseRevision {
        WarehouseRevision::new(
            WarehouseRevisionId::new("rev-1"),
            WarehouseRevisionData {
                warehouse_id: WarehouseId::new(warehouse_id),
                revision_no: 1,
                name: "北京仓".to_string(),
                region_code: erp_core::region::RegionCode::parse("110105").unwrap(),
                address: SensitiveText::new("cipher-a".to_string(), "a".repeat(64)).unwrap(),
                contact: SensitiveText::new("cipher-c".to_string(), "b".repeat(64)).unwrap(),
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
                change_reason: "创建".to_string(),
            },
        )
        .unwrap()
    }

    // 验证仓库代码去首尾空白，配置的初始状态和创建人正确写入实体。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let warehouse = Warehouse::new(WarehouseId::new("wh-1"), data(), "admin-1").unwrap();

        assert_eq!(warehouse.warehouse_code, "WH-BJ-001");
        assert!(warehouse.is_active());
        assert_eq!(warehouse.stable.created_by, "admin-1");
        assert_eq!(warehouse.inbound_handler_user_id.as_deref(), Some("inbound-1"));
        assert_eq!(warehouse.outbound_handler_user_id.as_deref(), Some("outbound-1"));
    }

    // 验证清理后为空或超过长度上限的仓库代码均不能建立稳定身份。
    #[test]
    fn new_rejects_empty_and_overlong_code() {
        let empty = WarehouseData::new("  ")
            .with_inbound_handler_user_id("inbound-1")
            .with_outbound_handler_user_id("outbound-1");
        assert!(Warehouse::new(WarehouseId::new("wh-1"), empty, "admin-1").is_err());

        let overlong = WarehouseData::new("w".repeat(65))
            .with_inbound_handler_user_id("inbound-1")
            .with_outbound_handler_user_id("outbound-1");
        assert!(Warehouse::new(WarehouseId::new("wh-1"), overlong, "admin-1").is_err());
    }

    // 验证仅给稳定代码创建时默认启用，且不会预设任何经办人。
    #[test]
    fn warehouse_data_constructor_sets_code_only() {
        let data = WarehouseData::new("WH-BJ-001");
        assert_eq!(data.warehouse_code, "WH-BJ-001");
        assert_eq!(data.status, EnableStatus::Active);
        assert!(data.inbound_handler_user_id.is_none());
        let staffed = WarehouseData::new("WH-BJ-001").with_inbound_handler_user_id("inbound-1");
        assert_eq!(staffed.inbound_handler_user_id.as_deref(), Some("inbound-1"));
    }

    // 验证仓库、修订和 SKU 策略三类仓储筛选均默认首页每页 20 条。
    #[test]
    fn tier_c_warehouse_filters_default_to_first_page_size_20() {
        use crate::repository::{WarehouseFilter, WarehouseRevisionFilter};
        assert_eq!((WarehouseFilter::default().page, WarehouseFilter::default().page_size), (1, 20));
        assert_eq!(
            (WarehouseRevisionFilter::default().page, WarehouseRevisionFilter::default().page_size),
            (1, 20)
        );
    }

    // 验证资料更新可修改状态和记录修改人，但不改变仓库稳定代码。
    #[test]
    fn update_applies_status_and_preserves_code() {
        let mut warehouse = Warehouse::new(WarehouseId::new("wh-1"), data(), "admin-1").unwrap();

        warehouse
            .update(
                WarehouseUpdate {
                    status: Some(EnableStatus::Disabled),
                    inbound_handler_user_id: None,
                    outbound_handler_user_id: None,
                },
                "admin-2",
            )
            .unwrap();

        assert!(!warehouse.is_active());
        assert_eq!(warehouse.warehouse_code, "WH-BJ-001");
        assert_eq!(warehouse.stable.updated_by, "admin-2");
    }

    // 验证入库、仓发各按对应职责取经办人，允许两项配置同一账号，缺失或停用时不得回退。
    #[test]
    fn fulfillment_handlers_are_operation_specific_and_fail_closed() {
        let warehouse = Warehouse::new(WarehouseId::new("wh-1"), data(), "admin-1").unwrap();
        assert_eq!(
            warehouse.fulfillment_handler(WarehouseFulfillmentOperation::Receipt).unwrap(),
            "inbound-1"
        );
        assert_eq!(
            warehouse.fulfillment_handler(WarehouseFulfillmentOperation::WarehouseShip).unwrap(),
            "outbound-1"
        );

        let missing = Warehouse::new(
            WarehouseId::new("wh-2"),
            WarehouseData::new(" WH-BJ-001 ").with_outbound_handler_user_id("same-user"),
            "admin-1",
        )
        .unwrap();
        assert!(missing.fulfillment_handler(WarehouseFulfillmentOperation::Receipt).is_err());
    }

    // 验证外仓修订不能成为当前修订，且修改只能匹配该实体的乐观锁版本。
    #[test]
    fn revision_and_version_matching_are_entity_rules() {
        let mut warehouse = Warehouse::new(WarehouseId::new("wh-1"), data(), "admin-1").unwrap();
        warehouse.base.version = 5;
        assert!(warehouse.matches_version(5));
        assert!(!warehouse.matches_version(4));

        warehouse.apply_revision(&revision("wh-1")).unwrap();
        assert_eq!(warehouse.stable.current_revision_id.as_deref(), Some("rev-1"));
        assert!(warehouse.apply_revision(&revision("wh-2")).is_err());
        assert_eq!(
            warehouse.stable.current_revision_id.as_deref(),
            Some("rev-1"),
            "归属失败不得改变当前修订"
        );
    }

    // 验证仓库启停迁移遵循实体状态图，允许双向切换及等状态重复操作。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
