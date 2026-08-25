//! `party`：企业主体稳定身份（数据模型 §6.2，页面：W03、W14）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::normalize_required_text;

use super::{PartyOwned, PartyRevision};

pub use crate::ids::PartyId;

/// 主体编号最大长度。
const PARTY_NO_MAX_LEN: usize = 64;
/// 统一社会信用代码固定长度（GB 32100-2015，18 位字母数字）。
const CREDIT_CODE_LEN: usize = 18;

/// 主体类型（§6.2：当前只使用企业组织）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    /// 企业组织。
    Enterprise,
}

impl PartyKind {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enterprise => "企业组织",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enterprise => "enterprise",
        }
    }
}

/// 主体启停状态（§6.2：启用/停用；对称状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl PartyStatus {
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

impl DocumentState for PartyStatus {
    /// 返回合法后继：启用 ⇄ 停用。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

/// 主体创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyData {
    /// 主体编号（全局唯一，创建后不可修改）。
    pub party_no: String,
    /// 主体类型。
    pub party_kind: PartyKind,
    /// 统一社会信用代码；允许历史数据为空。
    pub unified_credit_code: Option<String>,
    /// 启停状态。
    pub status: PartyStatus,
}

/// 主体更新数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyUpdate {
    /// 统一社会信用代码更新意图（`Unchanged` 保留、`Clear` 清除、`Set` 设置）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub unified_credit_code: FieldUpdate<String>,
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PartyStatus>,
}

/// 企业主体实体（稳定基础资料，§6.2）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Party {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<PartyStatus>,
    /// 主体编号（§4.1：编号一经形成正式事实不得复用）。
    pub party_no: String,
    /// 主体类型（创建后不可修改）。
    pub party_kind: PartyKind,
    /// 统一社会信用代码；允许历史数据为空，非空值规范化（大写）后全局唯一。
    pub unified_credit_code: Option<String>,
}

impl PartialEq for Party {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.party_no == other.party_no
            && self.party_kind == other.party_kind
            && self.unified_credit_code == other.unified_credit_code
    }
}

impl Eq for Party {}

impl Party {
    /// 创建企业主体。
    ///
    /// 完成 party_no 的校验与规范化（去首尾空白、非空、长度上限），
    /// 统一社会信用代码按规范化为大写并校验 18 位字母数字（允许为空）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PartyId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的主体实体。
    ///
    /// # 错误
    /// 当 party_no 为空/超长，或统一社会信用代码非空但不是 18 位字母数字时返回错误。
    pub fn new(id: PartyId, data: PartyData, created_by: impl Into<String>) -> Result<Self> {
        let party_no = normalize_required_text(
            data.party_no,
            "主体编号不能为空",
            PARTY_NO_MAX_LEN,
            "主体编号过长",
        )?;
        let unified_credit_code = normalize_credit_code(data.unified_credit_code)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            party_no,
            party_kind: data.party_kind,
            unified_credit_code,
        })
    }

    /// 更新企业主体。
    ///
    /// `party_no` 与 `party_kind` 是稳定身份，不允许在通用更新中修改；
    /// 状态迁移按固定状态机校验（§13.3）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当统一社会信用代码格式非法或状态迁移非法时返回错误。
    pub fn update(&mut self, update: PartyUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_credit_code(update.unified_credit_code)?;
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断主体是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 校验主体乐观锁版本。
    ///
    /// # 参数
    /// * `expected` - 客户端期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前版本与期望版本不一致时返回错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version == expected {
            return Ok(());
        }
        Err(Error::from("数据已被其他请求修改，请刷新后重试"))
    }

    /// 从已加载修订中解析当前生效修订。
    ///
    /// 当前指针必须存在、命中修订 ID，且修订必须属于当前 Party；这些
    /// 条件共同保护稳定主体与不可变修订链的一致性。
    ///
    /// # 参数
    /// * `revisions` - 已加载的主体修订集合
    ///
    /// # 返回
    /// 返回当前生效修订引用。
    ///
    /// # 错误
    /// 当前修订指针缺失、目标修订不存在或修订属于其他 Party 时返回错误。
    pub fn current_revision<'a>(&self, revisions: &'a [PartyRevision]) -> Result<&'a PartyRevision> {
        let revision_id = self
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::from("主体缺少当前名称修订"))?;
        let revision = revisions
            .iter()
            .find(|revision| revision.base.id == revision_id)
            .ok_or_else(|| Error::from("主体当前名称修订不存在"))?;
        revision.ensure_party(&PartyId::new(self.base.id.clone()))?;
        Ok(revision)
    }

    /// 应用统一社会信用代码更新。
    ///
    /// # 参数
    /// * `update` - 代码更新意图
    ///
    /// # 错误
    /// 当代码非空但不是 18 位字母数字时返回错误。
    fn apply_credit_code(&mut self, update: FieldUpdate<String>) -> Result<()> {
        match update {
            FieldUpdate::Unchanged => {}
            FieldUpdate::Clear => self.unified_credit_code = None,
            FieldUpdate::Set(value) => self.unified_credit_code = normalize_credit_code(Some(value))?,
        }
        Ok(())
    }

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选目标状态
    ///
    /// # 错误
    /// 目标状态不在固定状态机后继中时返回错误。
    fn apply_status(&mut self, status: Option<PartyStatus>) -> Result<()> {
        if let Some(to) = status {
            ensure_transition(self.stable.status, to)?;
            self.stable.status = to;
        }
        Ok(())
    }
}

/// 规范化统一社会信用代码。
///
/// 空值（含全空白）规范化为 `None`；非空值去首尾空白、转大写后必须为
/// 18 位字母数字（GB 32100-2015）。
///
/// # 参数
/// * `value` - 原始输入
///
/// # 返回
/// 返回规范化后的代码或 `None`。
///
/// # 错误
/// 当非空值不是 18 位字母数字时返回错误。
fn normalize_credit_code(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_uppercase();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() != CREDIT_CODE_LEN || !value.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(Error::from("统一社会信用代码必须是 18 位字母数字"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::{normalize_credit_code, Party, PartyData, PartyKind, PartyStatus, PartyUpdate};
    use crate::common::state::assert_adjacency_closed;
    use crate::field_update::FieldUpdate;
    use crate::ids::{PartyId, PartyRevisionId};
    use crate::party::{PartyRevision, PartyRevisionData};

    fn party_data() -> PartyData {
        PartyData {
            party_no: " P-2026-001 ".to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: Some(" 91310000ma1bl4kw9x ".to_string()),
            status: PartyStatus::Active,
        }
    }

    /// happy path：编号去空白，信用代码规范化为大写 18 位。
    #[test]
    fn new_trims_and_normalizes() {
        let party = Party::new(PartyId::new("party-1"), party_data(), "admin-1").unwrap();
        assert_eq!(party.party_no, "P-2026-001");
        assert_eq!(party.unified_credit_code.as_deref(), Some("91310000MA1BL4KW9X"));
        assert_eq!(party.party_kind, PartyKind::Enterprise);
        assert!(party.is_active());
        assert_eq!(party.stable.created_by, "admin-1");
    }

    /// 失败路径：编号为空/超长，信用代码非法格式。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = PartyData {
            party_no: "   ".to_string(),
            ..party_data()
        };
        assert!(Party::new(PartyId::new("p"), blank_no, "admin-1").is_err());

        let overlong_no = PartyData {
            party_no: "x".repeat(65),
            ..party_data()
        };
        assert!(Party::new(PartyId::new("p"), overlong_no, "admin-1").is_err());

        let bad_code = PartyData {
            unified_credit_code: Some("12345".to_string()),
            ..party_data()
        };
        assert!(Party::new(PartyId::new("p"), bad_code, "admin-1").is_err());
    }

    /// 信用代码允许为空（历史数据），全空白规范化为 None。
    #[test]
    fn credit_code_may_be_empty() {
        assert_eq!(normalize_credit_code(None).unwrap(), None);
        assert_eq!(normalize_credit_code(Some("   ".to_string())).unwrap(), None);
        let party = Party::new(
            PartyId::new("party-2"),
            PartyData {
                unified_credit_code: None,
                ..party_data()
            },
            "admin-1",
        )
        .unwrap();
        assert_eq!(party.unified_credit_code, None);
    }

    /// 状态机：合法迁移（启用→停用）通过，非法迁移不存在（单一后继）。
    #[test]
    fn status_transitions_follow_fixed_matrix() {
        assert_adjacency_closed(&[PartyStatus::Active, PartyStatus::Disabled]);

        let mut party = Party::new(PartyId::new("party-3"), party_data(), "admin-1").unwrap();
        party
            .update(
                PartyUpdate {
                    unified_credit_code: FieldUpdate::Unchanged,
                    status: Some(PartyStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        assert!(!party.is_active());
        assert_eq!(party.stable.updated_by, "admin-2");
    }

    /// 更新：信用代码可设置/清除/保留，编号与类型不可修改（不在更新面）。
    #[test]
    fn update_applies_credit_code_and_status_only() {
        let mut party = Party::new(PartyId::new("party-4"), party_data(), "admin-1").unwrap();

        party
            .update(
                PartyUpdate {
                    unified_credit_code: FieldUpdate::Clear,
                    status: None,
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(party.unified_credit_code, None);

        party
            .update(
                PartyUpdate {
                    unified_credit_code: FieldUpdate::Set(" 91310000ma1bl4kw9x ".to_string()),
                    status: None,
                },
                "admin-3",
            )
            .unwrap();
        assert_eq!(party.unified_credit_code.as_deref(), Some("91310000MA1BL4KW9X"));

        let invalid = PartyUpdate {
            unified_credit_code: FieldUpdate::Set("bad".to_string()),
            status: None,
        };
        assert!(party.update(invalid, "admin-4").is_err());
    }

    /// 版本校验接受当前版本并拒绝过期版本。
    #[test]
    fn version_check_rejects_stale_expected_version() {
        let party = Party::new(PartyId::new("party-version"), party_data(), "admin-1").unwrap();
        assert!(party.ensure_version(1).is_ok());
        assert!(party.ensure_version(2).is_err());
    }

    /// 当前修订必须由指针命中且归属于当前 Party。
    #[test]
    fn current_revision_enforces_pointer_and_party_ownership() {
        let mut party = Party::new(PartyId::new("party-1"), party_data(), "admin-1").unwrap();
        let revision = PartyRevision::new(
            PartyRevisionId::new("revision-1"),
            PartyRevisionData {
                party_id: PartyId::new("party-1"),
                revision_no: 1,
                legal_name: "示例企业".to_string(),
                short_name: None,
                change_reason: "首版".to_string(),
            },
        )
        .unwrap();

        assert!(party.current_revision(std::slice::from_ref(&revision)).is_err());
        party.stable.current_revision_id = Some("revision-1".to_string());
        assert_eq!(
            party
                .current_revision(std::slice::from_ref(&revision))
                .unwrap()
                .base
                .id,
            "revision-1"
        );

        let foreign = PartyRevision::new(
            PartyRevisionId::new("revision-1"),
            PartyRevisionData {
                party_id: PartyId::new("party-2"),
                revision_no: 1,
                legal_name: "其他企业".to_string(),
                short_name: None,
                change_reason: "首版".to_string(),
            },
        )
        .unwrap();
        assert!(party.current_revision(&[foreign]).is_err());
    }

    /// 实体 BSON 往返（含 ID 与 StableBase）。
    #[test]
    fn bson_roundtrip() {
        let party = Party::new(PartyId::new("party-5"), party_data(), "admin-1").unwrap();
        let roundtrip: Party =
            bson::deserialize_from_document(bson::serialize_to_document(&party).unwrap()).unwrap();
        assert_eq!(roundtrip, party);
    }
}
