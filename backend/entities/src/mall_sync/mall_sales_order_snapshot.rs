//! `mall_sales_order_snapshot`：商城卡券销售单快照（数据模型 §6.13）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::{MallSalesSyncJobId, SalesOrderRevisionId, SourceSystemId};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::ExternalOrderKey;

/// 来源单号最大长度。
const ORDER_NO_MAX_LEN: usize = 128;
/// 商城当前状态码最大长度。
const STATUS_CODE_MAX_LEN: usize = 64;
/// 规范化快照归档最大长度（含白名单字段的完整快照）。
const SNAPSHOT_MAX_LEN: usize = 65536;
/// 内容指纹最大长度。
const CONTENT_HASH_MAX_LEN: usize = 128;
/// 原始报文引用最大长度。
const RAW_PAYLOAD_MAX_LEN: usize = 512;

/// 快照映射状态（数据模型 §6.13：待映射、已应用、差异、无变化）。
///
/// 固定状态机：待映射单向推进到已应用、差异或无变化；
/// 差异处理经由 `master_mapping_task`，不在快照上回退。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotMappingStatus {
    /// 待映射。
    Pending,
    /// 已应用（形成销售版本）。
    Applied,
    /// 差异（转人工核对）。
    Difference,
    /// 无变化（与当前版本指纹一致，只更新最近同步时间）。
    NoChange,
}

impl SnapshotMappingStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待映射",
            Self::Applied => "已应用",
            Self::Difference => "差异",
            Self::NoChange => "无变化",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::Difference => "difference",
            Self::NoChange => "no_change",
        }
    }
}

impl DocumentState for SnapshotMappingStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Applied, Self::Difference, Self::NoChange],
            Self::Applied | Self::Difference | Self::NoChange => &[],
        }
    }
}

/// 快照创建数据（数据模型 §6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesOrderSnapshotData {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 一期来源单号原值。
    pub external_order_no: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹（可选列，仅用于变更判断）。
    pub content_hash: Option<String>,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// 规范化外部快照归档。
    pub normalized_snapshot: String,
    /// 可选的加密原始报文引用。
    pub raw_payload_reference: Option<String>,
    /// ERP 实际观察时间。
    pub observed_at: Instant,
    /// 来源任务。
    pub sync_job_id: MallSalesSyncJobId,
}

/// 商城卡券销售单快照实体（数据模型 §6.13）。
///
/// 快照是历史记录，`update` 受限：内容字段创建后不可修改，只允许按固定
/// 状态机推进 `mapping_status`。`external_order_key` 由来源单号生成，
/// 与 `(source_system_id, external_order_key, source_updated_at)` 唯一约束
/// 配套；同一来源单收到更早 `source_updated_at` 的快照直接丢弃（§6.13），
/// 由 P3 在写入前判定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MallSalesOrderSnapshot {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 一期来源单号原值。
    pub external_order_no: String,
    /// 二进制比较键（来源单号去首尾空白后的 UTF-8 字节）。
    pub external_order_key: ExternalOrderKey,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹（可选列，仅用于变更判断）。
    pub content_hash: Option<String>,
    /// 商城当前状态码（ERP 不自行推进商城商业状态，§7.2）。
    pub source_status_code: String,
    /// 规范化外部快照归档。
    pub normalized_snapshot: String,
    /// 可选的加密原始报文引用。
    pub raw_payload_reference: Option<String>,
    /// ERP 实际观察时间。
    pub observed_at: Instant,
    /// 映射状态。
    pub mapping_status: SnapshotMappingStatus,
    /// 成功形成的销售版本。
    pub applied_sales_order_revision_id: Option<SalesOrderRevisionId>,
    /// 来源任务。
    pub sync_job_id: MallSalesSyncJobId,
}

impl MallSalesOrderSnapshot {
    /// 创建商城销售单快照。
    ///
    /// 完成来源单号、状态码、规范化快照的校验与规范化（去首尾空白、非空、
    /// 长度上限），并生成 `external_order_key`（只移除首尾空白，不做大小写
    /// 折叠）；快照创建即待映射。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallSalesOrderSnapshotId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的快照实体。
    ///
    /// # 错误
    /// 当必填文本为空或超长时返回错误。
    pub fn new(id: crate::ids::MallSalesOrderSnapshotId, data: MallSalesOrderSnapshotData) -> Result<Self> {
        let external_order_no = normalize_required_text(
            data.external_order_no,
            "来源单号不能为空",
            ORDER_NO_MAX_LEN,
            "来源单号过长",
        )?;
        let source_status_code = normalize_required_text(
            data.source_status_code,
            "商城状态码不能为空",
            STATUS_CODE_MAX_LEN,
            "商城状态码过长",
        )?;
        let normalized_snapshot = normalize_required_text(
            data.normalized_snapshot,
            "规范化快照不能为空",
            SNAPSHOT_MAX_LEN,
            "规范化快照过长",
        )?;
        let content_hash = normalize_optional_text(data.content_hash, "内容指纹", CONTENT_HASH_MAX_LEN)?;
        let raw_payload_reference =
            normalize_optional_text(data.raw_payload_reference, "原始报文引用", RAW_PAYLOAD_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_system_id: data.source_system_id,
            external_order_key: ExternalOrderKey::from_trimmed(&external_order_no),
            external_order_no,
            source_updated_at: data.source_updated_at,
            content_hash,
            source_status_code,
            normalized_snapshot,
            raw_payload_reference,
            observed_at: data.observed_at,
            mapping_status: SnapshotMappingStatus::Pending,
            applied_sales_order_revision_id: None,
            sync_job_id: data.sync_job_id,
        })
    }

    /// 判断传入来源更新时间是否早于当前快照。
    ///
    /// # 参数
    /// * `candidate_updated_at` - 待接收快照的来源更新时间
    ///
    /// # 返回
    /// 待接收时间早于当前快照时返回 `true`。
    pub fn supersedes_candidate(&self, candidate_updated_at: Instant) -> bool {
        self.source_updated_at > candidate_updated_at
    }

    /// 标记快照已应用。
    ///
    /// 指纹与当前销售版本不同且基础资料与唯一明细校验通过时，才形成新销售
    /// 版本（§8.4 第 2 条）；零条或多条卡券明细、金额解析失败、基础资料无法
    /// 映射均不得写错误应收或经营归属（§6.13，事务职责在 P3）。
    ///
    /// # 参数
    /// * `revision_id` - 成功形成的销售版本
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 快照已离开待映射状态时返回错误。
    pub fn mark_applied(&mut self, revision_id: SalesOrderRevisionId) -> Result<()> {
        ensure_transition(self.mapping_status, SnapshotMappingStatus::Applied)?;
        self.mapping_status = SnapshotMappingStatus::Applied;
        self.applied_sales_order_revision_id = Some(revision_id);
        Ok(())
    }

    /// 标记快照为差异。
    ///
    /// 同一来源单、同一 `source_updated_at` 重复推送不同内容时保留最新快照
    /// 并转人工核对（§6.13），差异经由 `master_mapping_task` 处理，不在
    /// 快照上猜测先后顺序。
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 快照已离开待映射状态时返回错误。
    pub fn mark_difference(&mut self) -> Result<()> {
        ensure_transition(self.mapping_status, SnapshotMappingStatus::Difference)?;
        self.mapping_status = SnapshotMappingStatus::Difference;
        Ok(())
    }

    /// 标记快照为无变化。
    ///
    /// 指纹与当前销售版本一致时只更新最近同步时间，不创建新销售版本
    /// （§6.13）。
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 快照已离开待映射状态时返回错误。
    pub fn mark_no_change(&mut self) -> Result<()> {
        ensure_transition(self.mapping_status, SnapshotMappingStatus::NoChange)?;
        self.mapping_status = SnapshotMappingStatus::NoChange;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::ids::{MallSalesOrderSnapshotId, SalesOrderRevisionId};

    fn snapshot_data() -> MallSalesOrderSnapshotData {
        MallSalesOrderSnapshotData {
            source_system_id: SourceSystemId::new("sys-mall"),
            external_order_no: " SO-2026-001 ".to_string(),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            content_hash: Some(" sha256:abc ".to_string()),
            source_status_code: " EFFECTIVE ".to_string(),
            normalized_snapshot: " {\"sell_order\":\"SO-2026-001\"} ".to_string(),
            raw_payload_reference: Some(" enc://raw-1 ".to_string()),
            observed_at: Instant::from_unix_secs(1_700_000_100),
            sync_job_id: MallSalesSyncJobId::new("j-1"),
        }
    }

    #[test]
    fn new_trims_and_computes_external_order_key() {
        let snapshot =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-1"), snapshot_data()).unwrap();

        assert_eq!(snapshot.external_order_no, "SO-2026-001");
        assert_eq!(snapshot.external_order_key.as_bytes(), b"SO-2026-001");
        assert_eq!(snapshot.source_status_code, "EFFECTIVE");
        assert_eq!(snapshot.normalized_snapshot, "{\"sell_order\":\"SO-2026-001\"}");
        assert_eq!(snapshot.content_hash.as_deref(), Some("sha256:abc"));
        assert_eq!(snapshot.raw_payload_reference.as_deref(), Some("enc://raw-1"));
        assert_eq!(snapshot.mapping_status, SnapshotMappingStatus::Pending);
        assert!(snapshot.applied_sales_order_revision_id.is_none());
    }

    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_no = MallSalesOrderSnapshotData {
            external_order_no: "   ".to_string(),
            ..snapshot_data()
        };
        assert!(MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-2"), empty_no).is_err());

        let overlong_status = MallSalesOrderSnapshotData {
            source_status_code: "x".repeat(STATUS_CODE_MAX_LEN + 1),
            ..snapshot_data()
        };
        assert!(MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-3"), overlong_status).is_err());

        let overlong_snapshot = MallSalesOrderSnapshotData {
            normalized_snapshot: "x".repeat(SNAPSHOT_MAX_LEN + 1),
            ..snapshot_data()
        };
        assert!(
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-4"), overlong_snapshot).is_err()
        );
    }

    #[test]
    fn stale_candidate_is_derived_from_source_update_time() {
        let snapshot =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("snap-stale"), snapshot_data())
                .unwrap();
        assert!(snapshot.supersedes_candidate(Instant::from_unix_secs(
            snapshot.source_updated_at.unix_secs() - 1
        )));
        assert!(!snapshot.supersedes_candidate(snapshot.source_updated_at));
    }

    #[test]
    fn mapping_status_advances_once_per_snapshot() {
        let mut snapshot =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-5"), snapshot_data()).unwrap();

        snapshot.mark_applied(SalesOrderRevisionId::new("rev-1")).unwrap();
        assert_eq!(snapshot.mapping_status, SnapshotMappingStatus::Applied);
        assert_eq!(
            snapshot.applied_sales_order_revision_id,
            Some(SalesOrderRevisionId::new("rev-1"))
        );

        assert!(snapshot.mark_no_change().is_err(), "已应用快照不可再改判");
    }

    #[test]
    fn difference_and_no_change_are_terminal() {
        let mut difference =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-6"), snapshot_data()).unwrap();
        difference.mark_difference().unwrap();
        assert_eq!(difference.mapping_status, SnapshotMappingStatus::Difference);
        assert!(difference
            .mark_applied(SalesOrderRevisionId::new("rev-1"))
            .is_err());

        let mut no_change =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-7"), snapshot_data()).unwrap();
        no_change.mark_no_change().unwrap();
        assert!(no_change.mark_difference().is_err());
    }

    #[test]
    fn key_derivation_is_case_sensitive() {
        let lower = MallSalesOrderSnapshot::new(
            MallSalesOrderSnapshotId::new("s-8"),
            MallSalesOrderSnapshotData {
                external_order_no: "so-1".to_string(),
                ..snapshot_data()
            },
        )
        .unwrap();
        assert_eq!(lower.external_order_key.as_bytes(), b"so-1");
        assert_ne!(lower.external_order_key, ExternalOrderKey::from_trimmed("SO-1"));
    }

    #[test]
    fn mapping_status_machine_is_directed() {
        assert!(ensure_transition(SnapshotMappingStatus::Pending, SnapshotMappingStatus::Applied).is_ok());
        assert!(ensure_transition(SnapshotMappingStatus::Pending, SnapshotMappingStatus::Difference).is_ok());
        assert!(ensure_transition(SnapshotMappingStatus::Pending, SnapshotMappingStatus::NoChange).is_ok());
        assert!(ensure_transition(SnapshotMappingStatus::Applied, SnapshotMappingStatus::Pending).is_err());
        assert!(
            ensure_transition(SnapshotMappingStatus::Difference, SnapshotMappingStatus::Applied).is_err()
        );
    }

    #[test]
    fn mapping_status_serde_uses_stable_codes() {
        assert_eq!(
            serde_json::to_string(&SnapshotMappingStatus::NoChange).unwrap(),
            "\"no_change\""
        );
        assert_eq!(
            serde_json::to_string(&SnapshotMappingStatus::Applied).unwrap(),
            "\"applied\""
        );
        assert_eq!(SnapshotMappingStatus::Difference.label(), "差异");
    }

    #[test]
    fn bson_wire_roundtrip_persists_external_order_key_as_binary() {
        let snapshot =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-9"), snapshot_data()).unwrap();
        let bytes = bson::serialize_to_vec(&snapshot).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();

        let stored = wire_doc.get("external_order_key").unwrap();
        let bson::Bson::Binary(binary) = stored else {
            panic!("external_order_key 必须以 BSON Binary 持久化，实际为 {stored:?}");
        };
        assert_eq!(binary.bytes, b"SO-2026-001");

        let back: MallSalesOrderSnapshot = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, snapshot);
    }

    #[test]
    fn json_roundtrip_preserves_entity() {
        let snapshot =
            MallSalesOrderSnapshot::new(MallSalesOrderSnapshotId::new("s-10"), snapshot_data()).unwrap();
        let json = serde_json::to_string(&snapshot).unwrap();
        let back: MallSalesOrderSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snapshot);
    }
}
