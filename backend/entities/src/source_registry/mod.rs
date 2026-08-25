//! 域 D01 `source_registry`：source_system、external_identity_map、external_identity_target（页面：W17、W29）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common::StableBase` 基元。
//! 字段字典与唯一约束见数据模型 §6.1；公共字段归属按 §4.3 判定：
//! - `source_system` 是「稳定基础资料」→ 组合 [`crate::common::StableBase`]；
//! - `external_identity_map` / `external_identity_target` 是来源身份注册表，
//!   不属基础资料或正式事实 → 只用 `BaseModel` 持久化元数据，状态与审计字段按
//!   §6.1 各自建模（`mapping_status` / `mapped_at` / `mapped_by` /
//!   `approved_at` / `approved_by`），不硬套 StableBase 的 `status`/`created_by` 语义。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::stable::StableBase;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{ExternalIdentityMapId, ExternalIdentityTargetId, SourceSystemId};

/// 来源系统代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 名称最大长度。
const NAME_MAX_LEN: usize = 64;
/// 外部 ID 最大长度。
const EXTERNAL_ID_MAX_LEN: usize = 256;
/// 内部对象 ID 最大长度。
const INTERNAL_OBJECT_ID_MAX_LEN: usize = 128;
/// 操作人标识（映射责任人/确认人）最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 来源系统类型（数据模型 §6.1：`ERP`、`MALL`、`SUPPLIER`）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum SourceSystemType {
    /// ERP 系统。
    Erp,
    /// 商城系统。
    Mall,
    /// 供应商连接系统。
    Supplier,
}

impl SourceSystemType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Erp => "ERP",
            Self::Mall => "商城",
            Self::Supplier => "供应商",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Erp => "ERP",
            Self::Mall => "MALL",
            Self::Supplier => "SUPPLIER",
        }
    }
}

/// 来源系统启停状态（数据模型 §6.1：启用/停用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSystemStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

/// 商城同步执行阶段（W17）。
///
/// 阶段是服务端持久化事实，不允许调用方在写命令中用常量绕过当前阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MallSyncStage {
    /// 第一期由商城开单，ERP 只读接收商业事实。
    FirstPhaseMallOwned,
    /// 一期入口已封存，W17 仅保留历史查询与证据追溯。
    Archived,
}

impl MallSyncStage {
    /// 返回持久化与协议使用的稳定代码。
    ///
    /// # 返回
    /// 返回 W17 约定的阶段代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FirstPhaseMallOwned => "FIRST_PHASE_MALL_OWNED",
            Self::Archived => "ARCHIVED",
        }
    }
}

impl SourceSystemStatus {
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

    /// 判断是否处于启用状态。
    ///
    /// # 返回
    /// 处于 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 外部对象类型（数据模型 §6.1：客户、供应商、销售单、商品、SKU、卡券类目、商城用户）。
///
/// 本枚举同时用作 `external_identity_target.internal_object_type` 的取值集合：
/// ERP 规范对象与来源对象类型出自同一对象目录；`MallUser` 实际只出现在来源侧。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExternalObjectType {
    /// 客户。
    Customer,
    /// 合同。
    Contract,
    /// 企业主体；W17 结算主体映射使用该规范对象类型。
    Party,
    /// 供应商。
    Supplier,
    /// 销售单。
    SalesOrder,
    /// 商品。
    Product,
    /// SKU。
    Sku,
    /// 卡券类目。
    VoucherCategory,
    /// 商城用户。
    MallUser,
}

impl ExternalObjectType {
    /// 返回对象类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Customer => "客户",
            Self::Contract => "合同",
            Self::Party => "企业主体",
            Self::Supplier => "供应商",
            Self::SalesOrder => "销售单",
            Self::Product => "商品",
            Self::Sku => "SKU",
            Self::VoucherCategory => "卡券类目",
            Self::MallUser => "商城用户",
        }
    }

    /// 返回对象类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Customer => "customer",
            Self::Contract => "contract",
            Self::Party => "party",
            Self::Supplier => "supplier",
            Self::SalesOrder => "sales_order",
            Self::Product => "product",
            Self::Sku => "sku",
            Self::VoucherCategory => "voucher_category",
            Self::MallUser => "mall_user",
        }
    }
}

/// 映射状态（数据模型 §6.1：已映射、待确认、冲突、停用；固定枚举，无文档状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    /// 已映射。
    Mapped,
    /// 待确认。
    Pending,
    /// 冲突。
    Conflict,
    /// 停用。
    Disabled,
}

impl MappingStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mapped => "已映射",
            Self::Pending => "待确认",
            Self::Conflict => "冲突",
            Self::Disabled => "停用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mapped => "mapped",
            Self::Pending => "pending",
            Self::Conflict => "conflict",
            Self::Disabled => "disabled",
        }
    }
}

/// 映射目标关系角色（数据模型 §6.1：`PRIMARY`、`COMPONENT`、`MERGED_INTO`、`REVISION_SOURCE`）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationRole {
    /// 主身份：根业务身份同一时点只允许一个有效 PRIMARY 目标。
    Primary,
    /// 组成部分：来源对象拆成多个规范对象时使用多个 COMPONENT 目标。
    Component,
    /// 合并入：多个来源对象合并为同一 ERP 对象时使用。
    MergedInto,
    /// 修订来源：来源版本追溯，不得通过覆盖旧目标丢失映射历史。
    RevisionSource,
}

impl RelationRole {
    /// 返回关系角色的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Primary => "主身份",
            Self::Component => "组成部分",
            Self::MergedInto => "合并入",
            Self::RevisionSource => "修订来源",
        }
    }

    /// 返回关系角色的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Primary => "PRIMARY",
            Self::Component => "COMPONENT",
            Self::MergedInto => "MERGED_INTO",
            Self::RevisionSource => "REVISION_SOURCE",
        }
    }
}

/// 映射目标状态（数据模型 §6.1：待确认、有效、失效、冲突；固定枚举，无文档状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStatus {
    /// 待确认。
    Pending,
    /// 有效。
    Active,
    /// 失效。
    Expired,
    /// 冲突。
    Conflict,
}

impl TargetStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待确认",
            Self::Active => "有效",
            Self::Expired => "失效",
            Self::Conflict => "冲突",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Conflict => "conflict",
        }
    }
}

/// 外部身份二进制比较键。
///
/// 由 [`ExternalIdentityMap::external_id_key`] 生成：对来源原值移除首尾空白后取
/// UTF-8 字节，**保留大小写**、不做 Unicode 兼容折叠或数值化（数据模型 §6.1：
/// `ABC` 与 `abc` 是两个合法的不同来源身份）。BSON 形态固定为 `Binary`
/// （Generic subtype），不受数据库默认排序规则影响；唯一索引直接建在字节上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdKey(Vec<u8>);

impl ExternalIdKey {
    /// 构造二进制比较键。
    ///
    /// # 参数
    /// * `bytes` - 规范化后的 UTF-8 字节
    ///
    /// # 返回
    /// 返回比较键实例。
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// 返回比较键的字节切片。
    ///
    /// # 返回
    /// 返回规范化后的 UTF-8 字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 返回用于 MongoDB 唯一索引与等值筛选的 BSON 二进制值。
    ///
    /// # 返回
    /// 返回 Generic subtype 的 BSON 二进制，与实体持久化形态一致。
    pub fn to_bson_binary(&self) -> bson::Binary {
        bson::Binary {
            subtype: bson::spec::BinarySubtype::Generic,
            bytes: self.0.clone(),
        }
    }
}

impl fmt::Display for ExternalIdKey {
    /// 以 UTF-8 字符串形式展示比较键（用于日志与调试）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&String::from_utf8_lossy(&self.0))
    }
}

impl Serialize for ExternalIdKey {
    /// 序列化比较键：human-readable（JSON）输出字节数组；
    /// 非 human-readable（MongoDB 驱动）委托 `bson::Binary` 输出真正的 BSON 二进制。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_bytes(&self.0)
        } else {
            self.to_bson_binary().serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for ExternalIdKey {
    /// 反序列化比较键：JSON 形态接受字节数组；BSON 形态接受 `Binary` 变体。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_seq(ExternalIdKeyVisitor)
        } else {
            let binary = bson::Binary::deserialize(deserializer)?;
            Ok(Self(binary.bytes))
        }
    }
}

/// JSON 形态（字节数组）的比较键访问器。
struct ExternalIdKeyVisitor;

impl<'de> Visitor<'de> for ExternalIdKeyVisitor {
    type Value = ExternalIdKey;

    /// 描述期望的 JSON 形态。
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("外部身份比较键的 UTF-8 字节序列")
    }

    /// 从字节序列构造比较键。
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
        let mut bytes = Vec::new();
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        Ok(ExternalIdKey(bytes))
    }

    /// 接受 bson 以 human-readable 形态暴露的二进制值（`bson::deserialize_from_document` 默认模式）。
    fn visit_byte_buf<E>(self, bytes: Vec<u8>) -> std::result::Result<Self::Value, E> {
        Ok(ExternalIdKey(bytes))
    }
}

/// 来源系统创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSystemData {
    /// 稳定代码（唯一，如 `ERP`、目标商城或供应商连接所属系统）。
    pub code: String,
    /// 系统类型。
    pub system_type: SourceSystemType,
    /// 显示名称。
    pub name: String,
    /// 启停状态。
    pub status: SourceSystemStatus,
    /// 商城同步执行阶段；仅 `MALL` 来源必填，其余来源禁止设置。
    pub mall_sync_stage: Option<MallSyncStage>,
}

/// 来源系统更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SourceSystemUpdate {
    /// 显示名称；`None` 表示不修改。
    pub name: Option<String>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<SourceSystemStatus>,
    /// 商城同步执行阶段；仅 `MALL` 来源允许修改。
    pub mall_sync_stage: Option<MallSyncStage>,
}

/// 来源系统实体（稳定基础资料，数据模型 §6.1）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SourceSystem {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SourceSystemStatus>,
    /// 稳定代码（创建后不可修改）。
    pub code: String,
    /// 系统类型。
    pub system_type: SourceSystemType,
    /// 显示名称。
    pub name: String,
    /// 商城同步执行阶段；仅 `MALL` 来源存在。
    pub mall_sync_stage: Option<MallSyncStage>,
}

impl PartialEq for SourceSystem {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.code == other.code
            && self.system_type == other.system_type
            && self.name == other.name
            && self.mall_sync_stage == other.mall_sync_stage
    }
}

impl Eq for SourceSystem {}

impl SourceSystem {
    /// 创建来源系统。
    ///
    /// 完成 code/name 的完整校验与规范化（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SourceSystemId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的来源系统实体。
    ///
    /// # 错误
    /// 当 code/name 为空、超长或含未规范化空白时返回错误。
    pub fn new(id: SourceSystemId, data: SourceSystemData, created_by: impl Into<String>) -> Result<Self> {
        ensure_mall_sync_stage(data.system_type, data.mall_sync_stage)?;
        let code = normalize_required_text(
            data.code,
            "来源系统代码不能为空",
            CODE_MAX_LEN,
            "来源系统代码过长",
        )?;
        let name = normalize_required_text(
            data.name,
            "来源系统名称不能为空",
            NAME_MAX_LEN,
            "来源系统名称过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            code,
            system_type: data.system_type,
            name,
            mall_sync_stage: data.mall_sync_stage,
        })
    }

    /// 更新来源系统。
    ///
    /// 复用 `new` 的校验规则；`code` 是稳定代码，不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当更新字段校验失败时返回错误。
    pub fn update(&mut self, update: SourceSystemUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_name(update.name)?;
        self.apply_status(update.status);
        if let Some(stage) = update.mall_sync_stage {
            ensure_mall_sync_stage(self.system_type, Some(stage))?;
            self.mall_sync_stage = Some(stage);
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断系统是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 应用名称更新。
    ///
    /// # 参数
    /// * `name` - 可选名称
    ///
    /// # 错误
    /// 当名称为空或超长时返回错误。
    fn apply_name(&mut self, name: Option<String>) -> Result<()> {
        if let Some(name) = name {
            self.name =
                normalize_required_text(name, "来源系统名称不能为空", NAME_MAX_LEN, "来源系统名称过长")?;
        }
        Ok(())
    }

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选状态
    fn apply_status(&mut self, status: Option<SourceSystemStatus>) {
        if let Some(status) = status {
            self.stable.status = status;
        }
    }
}

/// 校验来源类型与商城同步阶段的一致性。
fn ensure_mall_sync_stage(
    system_type: SourceSystemType,
    mall_sync_stage: Option<MallSyncStage>,
) -> Result<()> {
    match (system_type, mall_sync_stage) {
        (SourceSystemType::Mall, Some(_)) => Ok(()),
        (SourceSystemType::Mall, None) => Err(Error::from("MALL来源必须明确商城同步执行阶段")),
        (_, None) => Ok(()),
        (_, Some(_)) => Err(Error::from("只有MALL来源可以设置商城同步执行阶段")),
    }
}

/// 外部身份映射创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalIdentityMapData {
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 外部对象类型。
    pub object_type: ExternalObjectType,
    /// 来源稳定 ID 或单号原值（始终保留原值）。
    pub external_id: String,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 映射时间（秒级时间戳）；与 `mapped_by` 必须成对出现。
    pub mapped_at: Option<u64>,
    /// 映射责任人；与 `mapped_at` 必须成对出现。
    pub mapped_by: Option<String>,
}

/// 外部身份映射实体（数据模型 §6.1）。
///
/// `external_id_key` 是映射身份的规范化比较键，唯一约束
/// `(source_system_id, object_type, external_id_key)` 由唯一索引保证。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ExternalIdentityMap {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 外部对象类型。
    pub object_type: ExternalObjectType,
    /// 来源稳定 ID 或单号原值。
    pub external_id: String,
    /// 规范化二进制比较键（首尾去空白后 UTF-8 字节，保留大小写）。
    pub external_id_key: ExternalIdKey,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 映射时间（秒级时间戳）。
    pub mapped_at: Option<u64>,
    /// 映射责任人。
    pub mapped_by: Option<String>,
}

impl ExternalIdentityMap {
    /// 创建外部身份映射。
    ///
    /// 完成 external_id 的校验与规范化，并按数据模型 §6.1 的协议生成
    /// `external_id_key`（只移除首尾空白，不做大小写折叠）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ExternalIdentityMapId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的映射实体。
    ///
    /// # 错误
    /// 当 external_id 为空/超长，或 `mapped_at` 与 `mapped_by` 不同时出现时返回错误。
    pub fn new(id: ExternalIdentityMapId, data: ExternalIdentityMapData) -> Result<Self> {
        let external_id = normalize_required_text(
            data.external_id,
            "外部ID不能为空",
            EXTERNAL_ID_MAX_LEN,
            "外部ID过长",
        )?;
        let mapped_by = normalize_optional_text(data.mapped_by, "映射责任人", ACTOR_MAX_LEN)?;
        if data.mapped_at.is_some() != mapped_by.is_some() {
            return Err(Error::from("映射时间与映射责任人必须同时提供或同时省略"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_system_id: data.source_system_id,
            object_type: data.object_type,
            external_id: external_id.clone(),
            external_id_key: Self::external_id_key(&external_id),
            mapping_status: data.mapping_status,
            mapped_at: data.mapped_at,
            mapped_by,
        })
    }

    /// 生成外部身份比较键。
    ///
    /// 规范化规则（数据模型 §6.1）：只移除首尾空白，不做大小写折叠、
    /// Unicode 兼容折叠或数值化；`ABC` 与 `abc` 是两个不同的合法来源身份。
    ///
    /// # 参数
    /// * `external_id` - 来源原值
    ///
    /// # 返回
    /// 返回去除首尾空白后按 UTF-8 编码的字节。
    pub fn external_id_key(external_id: &str) -> ExternalIdKey {
        ExternalIdKey::new(external_id.trim().as_bytes().to_vec())
    }

    /// 将既有来源身份确认为已映射。
    ///
    /// 该方法只推进映射头状态；目标历史由 [`ExternalIdentityTarget`] 追加维护。
    pub fn confirm_mapping(&mut self, mapped_at: u64, mapped_by: String) -> Result<()> {
        let mapped_by =
            normalize_required_text(mapped_by, "映射责任人不能为空", ACTOR_MAX_LEN, "映射责任人过长")?;
        self.mapping_status = MappingStatus::Mapped;
        self.mapped_at = Some(mapped_at);
        self.mapped_by = Some(mapped_by);
        Ok(())
    }
}

/// 映射目标创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalIdentityTargetData {
    /// 来源稳定身份（映射实体 ID）。
    pub external_identity_map_id: ExternalIdentityMapId,
    /// ERP 规范对象类型。
    pub internal_object_type: ExternalObjectType,
    /// ERP 规范对象 ID。
    pub internal_object_id: String,
    /// 关系角色。
    pub relation_role: RelationRole,
    /// 映射生效时间（秒级时间戳）。
    pub valid_from: u64,
    /// 映射失效时间（秒级时间戳）；必须晚于 `valid_from`。
    pub valid_to: Option<u64>,
    /// 目标状态。
    pub status: TargetStatus,
    /// 业务确认时间（秒级时间戳）；与 `approved_by` 必须成对出现。
    pub approved_at: Option<u64>,
    /// 业务确认人；与 `approved_at` 必须成对出现。
    pub approved_by: Option<String>,
}

/// 映射目标实体（数据模型 §6.1）。
///
/// 目标表是「来源稳定身份 → ERP 规范对象」的可审计谱系；
/// 唯一约束 `(external_identity_map_id, internal_object_type, internal_object_id,
/// relation_role, valid_from)` 由唯一索引保证，禁止通过覆盖旧目标丢失映射历史。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ExternalIdentityTarget {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源稳定身份（映射实体 ID）。
    pub external_identity_map_id: ExternalIdentityMapId,
    /// ERP 规范对象类型。
    pub internal_object_type: ExternalObjectType,
    /// ERP 规范对象 ID。
    pub internal_object_id: String,
    /// 关系角色。
    pub relation_role: RelationRole,
    /// 映射生效时间（秒级时间戳）。
    pub valid_from: u64,
    /// 映射失效时间（秒级时间戳）。
    pub valid_to: Option<u64>,
    /// 目标状态。
    pub status: TargetStatus,
    /// 业务确认时间（秒级时间戳）。
    pub approved_at: Option<u64>,
    /// 业务确认人。
    pub approved_by: Option<String>,
}

impl ExternalIdentityTarget {
    /// 创建映射目标。
    ///
    /// 完成 internal_object_id 的校验与规范化，并强制两条不变式：
    /// `valid_to` 必须晚于 `valid_from`；`approved_at` 与 `approved_by` 必须成对。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ExternalIdentityTargetId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的目标实体。
    ///
    /// # 错误
    /// 当内部对象 ID 为空/超长、有效期倒挂或确认信息不完整时返回错误。
    pub fn new(id: ExternalIdentityTargetId, data: ExternalIdentityTargetData) -> Result<Self> {
        let internal_object_id = normalize_required_text(
            data.internal_object_id,
            "内部对象ID不能为空",
            INTERNAL_OBJECT_ID_MAX_LEN,
            "内部对象ID过长",
        )?;
        let approved_by = normalize_optional_text(data.approved_by, "确认人", ACTOR_MAX_LEN)?;
        if let Some(valid_to) = data.valid_to {
            if valid_to <= data.valid_from {
                return Err(Error::from("映射失效时间必须晚于生效时间"));
            }
        }
        if data.approved_at.is_some() != approved_by.is_some() {
            return Err(Error::from("确认时间与确认人必须同时提供或同时省略"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            external_identity_map_id: data.external_identity_map_id,
            internal_object_type: data.internal_object_type,
            internal_object_id,
            relation_role: data.relation_role,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            status: data.status,
            approved_at: data.approved_at,
            approved_by,
        })
    }

    /// 关闭当前有效目标，保留不可覆盖的谱系历史。
    ///
    /// # 错误
    /// 仅有效目标可关闭，且失效时间必须晚于生效时间。
    pub fn expire(&mut self, valid_to: u64) -> Result<()> {
        if self.status != TargetStatus::Active {
            return Err(Error::from("只有有效映射目标可以失效"));
        }
        if valid_to <= self.valid_from {
            return Err(Error::from("映射失效时间必须晚于生效时间"));
        }
        self.valid_to = Some(valid_to);
        self.status = TargetStatus::Expired;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExternalIdKey, ExternalIdentityMap, ExternalIdentityMapData, ExternalIdentityTarget,
        ExternalIdentityTargetData, ExternalObjectType, MallSyncStage, MappingStatus, RelationRole,
        SourceSystem, SourceSystemData, SourceSystemStatus, SourceSystemType, SourceSystemUpdate,
        TargetStatus,
    };
    use crate::ids::{ExternalIdentityMapId, ExternalIdentityTargetId, SourceSystemId};

    fn source_system_data() -> SourceSystemData {
        SourceSystemData {
            code: " ERP ".to_string(),
            system_type: SourceSystemType::Mall,
            name: " 目标商城 ".to_string(),
            status: SourceSystemStatus::Active,
            mall_sync_stage: Some(MallSyncStage::FirstPhaseMallOwned),
        }
    }

    fn map_data() -> ExternalIdentityMapData {
        ExternalIdentityMapData {
            source_system_id: SourceSystemId::new("sys-1"),
            object_type: ExternalObjectType::SalesOrder,
            external_id: " SO-2025-001 ".to_string(),
            mapping_status: MappingStatus::Pending,
            mapped_at: None,
            mapped_by: None,
        }
    }

    #[test]
    fn source_system_new_trims_and_validates_text_fields() {
        let system =
            SourceSystem::new(SourceSystemId::new("sys-1"), source_system_data(), "admin-1").unwrap();

        assert_eq!(system.code, "ERP");
        assert_eq!(system.name, "目标商城");
        assert_eq!(system.system_type, SourceSystemType::Mall);
        assert_eq!(system.mall_sync_stage, Some(MallSyncStage::FirstPhaseMallOwned));
        assert_eq!(system.stable.status(), SourceSystemStatus::Active);
        assert_eq!(system.stable.created_by, "admin-1");
        assert_eq!(system.stable.updated_by, "admin-1");
        assert!(system.is_active());
    }

    #[test]
    fn source_system_new_rejects_empty_code() {
        let data = SourceSystemData {
            code: "   ".to_string(),
            ..source_system_data()
        };
        assert!(SourceSystem::new(SourceSystemId::new("sys-1"), data, "admin-1").is_err());
    }

    #[test]
    fn source_system_new_rejects_overlong_code_and_name() {
        let overlong_code = SourceSystemData {
            code: "x".repeat(65),
            ..source_system_data()
        };
        assert!(SourceSystem::new(SourceSystemId::new("sys-1"), overlong_code, "admin-1").is_err());

        let overlong_name = SourceSystemData {
            name: "n".repeat(65),
            ..source_system_data()
        };
        assert!(SourceSystem::new(SourceSystemId::new("sys-1"), overlong_name, "admin-1").is_err());
    }

    #[test]
    fn source_system_update_applies_fields_and_touches_auditor() {
        let mut system =
            SourceSystem::new(SourceSystemId::new("sys-1"), source_system_data(), "admin-1").unwrap();

        system
            .update(
                SourceSystemUpdate {
                    name: Some(" 新名称 ".to_string()),
                    status: Some(SourceSystemStatus::Disabled),
                    mall_sync_stage: Some(MallSyncStage::Archived),
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(system.name, "新名称");
        assert!(!system.is_active());
        assert_eq!(system.mall_sync_stage, Some(MallSyncStage::Archived));
        assert_eq!(system.stable.updated_by, "admin-2");
        assert_eq!(system.stable.created_by, "admin-1", "touch 不修改创建人");
    }

    #[test]
    fn source_system_update_rejects_blank_name() {
        let mut system =
            SourceSystem::new(SourceSystemId::new("sys-1"), source_system_data(), "admin-1").unwrap();

        assert!(system
            .update(
                SourceSystemUpdate {
                    name: Some("   ".to_string()),
                    status: None,
                    mall_sync_stage: None,
                },
                "admin-2",
            )
            .is_err());
    }

    #[test]
    fn source_system_requires_explicit_stage_only_for_mall() {
        let missing_mall_stage = SourceSystemData {
            mall_sync_stage: None,
            ..source_system_data()
        };
        assert!(SourceSystem::new(SourceSystemId::new("mall-1"), missing_mall_stage, "admin-1",).is_err());

        let erp_with_mall_stage = SourceSystemData {
            system_type: SourceSystemType::Erp,
            ..source_system_data()
        };
        assert!(SourceSystem::new(SourceSystemId::new("erp-1"), erp_with_mall_stage, "admin-1",).is_err());
    }

    #[test]
    fn external_id_key_trims_outer_whitespace_only_and_keeps_case() {
        let key = ExternalIdentityMap::external_id_key("  SO-1  ");
        assert_eq!(key.as_bytes(), b"SO-1");

        let upper = ExternalIdentityMap::external_id_key("ABC");
        let lower = ExternalIdentityMap::external_id_key("abc");
        assert_ne!(upper, lower, "ABC 与 abc 是两个不同的合法来源身份");
        assert_eq!(upper.as_bytes(), b"ABC");

        let padded = ExternalIdentityMap::external_id_key("  ABC  ");
        assert_eq!(upper, padded, "首尾空白不参与比较键");

        let inner_space = ExternalIdentityMap::external_id_key("AB C");
        assert_ne!(upper, inner_space, "中间空白保留，不做折叠");
    }

    #[test]
    fn map_new_computes_key_and_validates_mapped_pair() {
        let map = ExternalIdentityMap::new(ExternalIdentityMapId::new("map-1"), map_data()).unwrap();

        assert_eq!(map.external_id, "SO-2025-001");
        assert_eq!(map.external_id_key, ExternalIdKey::new(b"SO-2025-001".to_vec()));
        assert_eq!(map.mapping_status, MappingStatus::Pending);

        let half_pair = ExternalIdentityMapData {
            mapped_at: Some(1_700_000_000),
            mapped_by: None,
            ..map_data()
        };
        assert!(ExternalIdentityMap::new(ExternalIdentityMapId::new("map-2"), half_pair).is_err());
    }

    #[test]
    fn target_new_validates_validity_window_and_approval_pair() {
        let data = ExternalIdentityTargetData {
            external_identity_map_id: ExternalIdentityMapId::new("map-1"),
            internal_object_type: ExternalObjectType::SalesOrder,
            internal_object_id: " SO-2025-001 ".to_string(),
            relation_role: RelationRole::Primary,
            valid_from: 1_700_000_000,
            valid_to: Some(1_700_086_400),
            status: TargetStatus::Pending,
            approved_at: None,
            approved_by: None,
        };
        let target =
            ExternalIdentityTarget::new(ExternalIdentityTargetId::new("target-1"), data.clone()).unwrap();

        assert_eq!(target.internal_object_id, "SO-2025-001");
        assert_eq!(target.relation_role, RelationRole::Primary);

        let reversed_window = ExternalIdentityTargetData {
            valid_to: Some(1_699_913_600),
            ..data.clone()
        };
        assert!(
            ExternalIdentityTarget::new(ExternalIdentityTargetId::new("target-2"), reversed_window).is_err()
        );

        let half_approval = ExternalIdentityTargetData {
            approved_at: Some(1_700_000_000),
            approved_by: None,
            ..data.clone()
        };
        assert!(
            ExternalIdentityTarget::new(ExternalIdentityTargetId::new("target-3"), half_approval).is_err()
        );
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&SourceSystemType::Mall).unwrap(),
            "\"MALL\""
        );
        assert_eq!(
            serde_json::to_string(&RelationRole::MergedInto).unwrap(),
            "\"MERGED_INTO\""
        );
        assert_eq!(
            serde_json::to_string(&MappingStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&ExternalObjectType::VoucherCategory).unwrap(),
            "\"voucher_category\""
        );

        assert_eq!(SourceSystemType::Erp.label(), "ERP");
        assert_eq!(SourceSystemType::Supplier.label(), "供应商");
        assert_eq!(SourceSystemStatus::Active.label(), "启用");
        assert_eq!(MappingStatus::Conflict.label(), "冲突");
        assert_eq!(TargetStatus::Expired.label(), "失效");
        assert_eq!(RelationRole::RevisionSource.label(), "修订来源");
        assert_eq!(ExternalObjectType::MallUser.label(), "商城用户");
    }

    #[test]
    fn bson_wire_roundtrip_persists_external_id_key_as_binary() {
        let map = ExternalIdentityMap::new(ExternalIdentityMapId::new("map-1"), map_data()).unwrap();
        // 非 human-readable 的 to_vec/from_slice 精确对应 mongodb 驱动持久化路径
        //（money.rs 同款约定；human_readable(false) builder 在 bson 2.15 已废弃）。
        let bytes = bson::serialize_to_vec(&map).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();

        let stored = wire_doc.get("external_id_key").unwrap();
        let bson::Bson::Binary(binary) = stored else {
            panic!("external_id_key 必须以 BSON Binary 持久化，实际为 {stored:?}");
        };
        assert_eq!(binary.bytes, b"SO-2025-001");

        let back: ExternalIdentityMap = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, map);
    }

    #[test]
    fn entities_roundtrip_through_bson_including_ids() {
        let system =
            SourceSystem::new(SourceSystemId::new("sys-1"), source_system_data(), "admin-1").unwrap();
        let roundtrip: SourceSystem =
            bson::deserialize_from_document(bson::serialize_to_document(&system).unwrap()).unwrap();
        assert_eq!(roundtrip, system);

        let target = ExternalIdentityTarget::new(
            ExternalIdentityTargetId::new("target-1"),
            ExternalIdentityTargetData {
                external_identity_map_id: ExternalIdentityMapId::new("map-1"),
                internal_object_type: ExternalObjectType::Sku,
                internal_object_id: "sku-1".to_string(),
                relation_role: RelationRole::Component,
                valid_from: 1_700_000_000,
                valid_to: None,
                status: TargetStatus::Pending,
                approved_at: None,
                approved_by: None,
            },
        )
        .unwrap();
        let roundtrip: ExternalIdentityTarget =
            bson::deserialize_from_document(bson::serialize_to_document(&target).unwrap()).unwrap();
        assert_eq!(roundtrip, target);
    }

    #[test]
    fn external_id_key_json_roundtrip_uses_byte_array() {
        let key = ExternalIdKey::new(b"SO-1".to_vec());
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "[83,79,45,49]");
        let back: ExternalIdKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, key);
    }
}
