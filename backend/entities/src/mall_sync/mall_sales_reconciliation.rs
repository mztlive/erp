//! `mall_sales_reconciliation_job` 与 `mall_sales_reconciliation_item`：
//! 商城卡券销售单核对作业与差异明细（数据模型 §6.13）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    MallSalesReconciliationJobId, MallSalesSyncJobId, SalesOrderId, SalesOrderRevisionId, SourceSystemId,
};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::ExternalOrderKey;

/// 核对批次号最大长度。
const JOB_NO_MAX_LEN: usize = 64;
/// 来源单号最大长度。
const ORDER_NO_MAX_LEN: usize = 128;
/// 商城状态码与内容指纹最大长度。
const CODE_MAX_LEN: usize = 128;
/// 处理结论最大长度。
const RESOLUTION_MAX_LEN: usize = 1024;
/// 处理人标识最大长度。
const RESOLVER_MAX_LEN: usize = 128;

/// 核对作业状态（数据模型 §6.13：运行中、完成、有差异、失败）。
///
/// 固定状态机：运行中单向推进到完成、有差异或失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationJobStatus {
    /// 运行中。
    Running,
    /// 完成（无差异）。
    Completed,
    /// 有差异（差异明细已持久化）。
    HasDifference,
    /// 失败。
    Failed,
}

impl ReconciliationJobStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "运行中",
            Self::Completed => "完成",
            Self::HasDifference => "有差异",
            Self::Failed => "失败",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::HasDifference => "has_difference",
            Self::Failed => "failed",
        }
    }
}

impl DocumentState for ReconciliationJobStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Running => &[Self::Completed, Self::HasDifference, Self::Failed],
            Self::Completed | Self::HasDifference | Self::Failed => &[],
        }
    }
}

/// 核对作业创建数据（数据模型 §6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesReconciliationJobData {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 核对批次号（唯一）。
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 任务开始时间。
    pub started_at: Instant,
}

/// 商城卡券销售单核对作业实体（数据模型 §6.13）。
///
/// 一期按月全量或按单抽查核对使用专用强类型表，不等到二期通用接口对账才
/// 启用；核对只生成差异和任务，不直接覆盖来源快照、ERP 销售版本、应收或
/// 经营事实（§6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MallSalesReconciliationJob {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 核对批次号（创建后不可修改）。
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 双方数量：商城清单数量。
    pub source_count: u64,
    /// 双方数量：ERP 数量。
    pub erp_count: u64,
    /// 双方数量：差异数量。
    pub difference_count: u64,
    /// 作业状态。
    pub status: ReconciliationJobStatus,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
}

impl MallSalesReconciliationJob {
    /// 创建核对作业。
    ///
    /// 完成核对批次号的校验与规范化（去首尾空白、非空、长度上限）；
    /// 作业创建即运行中，计数为零。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallSalesReconciliationJobId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的核对作业实体。
    ///
    /// # 错误
    /// 批次号为空或超长时返回错误。
    pub fn new(id: MallSalesReconciliationJobId, data: MallSalesReconciliationJobData) -> Result<Self> {
        let job_no = normalize_required_text(
            data.job_no,
            "核对批次号不能为空",
            JOB_NO_MAX_LEN,
            "核对批次号过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_system_id: data.source_system_id,
            job_no,
            source_list_as_of: data.source_list_as_of,
            source_count: 0,
            erp_count: 0,
            difference_count: 0,
            status: ReconciliationJobStatus::Running,
            started_at: data.started_at,
            finished_at: None,
        })
    }

    /// 登记双方数量。
    ///
    /// # 参数
    /// * `source_count` - 商城清单数量
    /// * `erp_count` - ERP 数量
    /// * `difference_count` - 差异数量（对称差，不超过两侧数量之和）
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 差异数量超过两侧数量之和时返回错误。
    pub fn record_counts(&mut self, source_count: u64, erp_count: u64, difference_count: u64) -> Result<()> {
        if difference_count > source_count + erp_count {
            return Err(Error::from("差异数量不能超过两侧数量之和"));
        }
        self.source_count = source_count;
        self.erp_count = erp_count;
        self.difference_count = difference_count;
        Ok(())
    }

    /// 完成核对作业并登记结束时间。
    ///
    /// 完成表示双方一致（差异为零），有差异要求差异明细已持久化
    /// （差异数量大于零）。
    ///
    /// # 参数
    /// * `outcome` - 完成、有差异或失败
    /// * `finished_at` - 任务结束时间
    ///
    /// # 返回
    /// 完成操作返回 `Ok(())`。
    ///
    /// # 错误
    /// 非运行中状态，或完成/有差异结果与差异计数矛盾时返回错误。
    pub fn finish(&mut self, outcome: ReconciliationJobStatus, finished_at: Instant) -> Result<()> {
        ensure_transition(self.status, outcome)?;
        match outcome {
            ReconciliationJobStatus::Completed if self.difference_count > 0 => {
                return Err(Error::from("存在差异计数不能记为完成"));
            }
            ReconciliationJobStatus::HasDifference if self.difference_count == 0 => {
                return Err(Error::from("差异计数为零不能记为有差异"));
            }
            _ => {}
        }
        self.status = outcome;
        self.finished_at = Some(finished_at);
        Ok(())
    }
}

/// 差异类型（数据模型 §6.13：商城缺失、ERP 缺失、状态差异、内容指纹差异、重复身份）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationDifferenceType {
    /// 商城缺失（ERP 存在而商城清单不含）。
    MallMissing,
    /// ERP 缺失（商城存在而 ERP 无对应正式单）。
    ErpMissing,
    /// 状态差异。
    StatusDifference,
    /// 内容指纹差异。
    ContentFingerprintDifference,
    /// 重复身份。
    DuplicateIdentity,
}

impl ReconciliationDifferenceType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::MallMissing => "商城缺失",
            Self::ErpMissing => "ERP 缺失",
            Self::StatusDifference => "状态差异",
            Self::ContentFingerprintDifference => "内容指纹差异",
            Self::DuplicateIdentity => "重复身份",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MallMissing => "mall_missing",
            Self::ErpMissing => "erp_missing",
            Self::StatusDifference => "status_difference",
            Self::ContentFingerprintDifference => "content_fingerprint_difference",
            Self::DuplicateIdentity => "duplicate_identity",
        }
    }
}

/// 差异明细状态（数据模型 §6.13：待处理、补拉中、已解决、确认无误）。
///
/// 固定状态机：待处理可进入补拉中或直接解决/确认无误，补拉中单向推进到
/// 解决或确认无误；切换后停止新核对任务，历史批次和处理证据永久可查（§6.13）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationItemStatus {
    /// 待处理。
    Pending,
    /// 补拉中（按原来源身份发起单号补拉）。
    Backfilling,
    /// 已解决（人工处理完成）。
    Resolved,
    /// 确认无误（补拉后核对比对一致）。
    ConfirmedNoDifference,
}

impl ReconciliationItemStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::Backfilling => "补拉中",
            Self::Resolved => "已解决",
            Self::ConfirmedNoDifference => "确认无误",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Backfilling => "backfilling",
            Self::Resolved => "resolved",
            Self::ConfirmedNoDifference => "confirmed_no_difference",
        }
    }

    /// 判断差异明细是否已经形成不可逆结论。
    ///
    /// # 返回
    /// 已解决或确认无差异时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Resolved | Self::ConfirmedNoDifference)
    }
}

impl DocumentState for ReconciliationItemStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Backfilling, Self::Resolved, Self::ConfirmedNoDifference],
            Self::Backfilling => &[Self::Resolved, Self::ConfirmedNoDifference],
            Self::Resolved | Self::ConfirmedNoDifference => &[],
        }
    }
}

/// 差异明细创建数据（数据模型 §6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesReconciliationItemData {
    /// 所属核对作业。
    pub reconciliation_job_id: MallSalesReconciliationJobId,
    /// 来源单号。
    pub external_order_no: String,
    /// 商城清单值：商城当前状态码。
    pub source_status_code: String,
    /// 商城清单值：商城更新时间。
    pub source_updated_at: Instant,
    /// 商城清单值：内容指纹（可为空）。
    pub source_content_hash: Option<String>,
    /// ERP 当前正式值：销售单 ID。
    pub sales_order_id: Option<SalesOrderId>,
    /// ERP 当前正式值：销售版本 ID。
    pub erp_revision_id: Option<SalesOrderRevisionId>,
    /// ERP 当前正式值：内容指纹（可为空）。
    pub erp_content_hash: Option<String>,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
}

/// 商城卡券销售单核对差异明细实体（数据模型 §6.13）。
///
/// 商城缺失、ERP 缺失或指纹差异都持久化明细，并按原来源身份发起单号补拉
/// 或转 `work_item`；系统管理员不得手工补建另一张销售单（§6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MallSalesReconciliationItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属核对作业。
    pub reconciliation_job_id: MallSalesReconciliationJobId,
    /// 来源单号。
    pub external_order_no: String,
    /// 二进制比较键（来源单号去首尾空白后的 UTF-8 字节）。
    pub external_order_key: ExternalOrderKey,
    /// 商城清单值：商城当前状态码。
    pub source_status_code: String,
    /// 商城清单值：商城更新时间。
    pub source_updated_at: Instant,
    /// 商城清单值：内容指纹。
    pub source_content_hash: Option<String>,
    /// ERP 当前正式值：销售单 ID。
    pub sales_order_id: Option<SalesOrderId>,
    /// ERP 当前正式值：销售版本 ID。
    pub erp_revision_id: Option<SalesOrderRevisionId>,
    /// ERP 当前正式值：内容指纹。
    pub erp_content_hash: Option<String>,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
    /// 明细状态。
    pub status: ReconciliationItemStatus,
    /// 按单号补拉任务（可空）。
    pub single_order_sync_job_id: Option<MallSalesSyncJobId>,
    /// 人工处理结论。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
}

impl MallSalesReconciliationItem {
    /// 创建核对差异明细。
    ///
    /// 完成来源单号与状态码的校验与规范化（去首尾空白、非空、长度上限），
    /// 生成 `external_order_key`，并强制差异类型与 ERP 侧存在性的一致性：
    /// `ERP 缺失` 不得携带销售单 ID，`商城缺失`/`状态差异`/`内容指纹差异`
    /// 必须携带销售单 ID（双侧正式值参与比对）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallSalesReconciliationItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的差异明细实体（待处理）。
    ///
    /// # 错误
    /// 必填文本为空/超长，或差异类型与销售单 ID 的存在性不一致时返回错误。
    pub fn new(
        id: crate::ids::MallSalesReconciliationItemId,
        data: MallSalesReconciliationItemData,
    ) -> Result<Self> {
        let external_order_no = normalize_required_text(
            data.external_order_no,
            "来源单号不能为空",
            ORDER_NO_MAX_LEN,
            "来源单号过长",
        )?;
        let source_status_code = normalize_required_text(
            data.source_status_code,
            "商城状态码不能为空",
            CODE_MAX_LEN,
            "商城状态码过长",
        )?;
        let source_content_hash =
            normalize_optional_text(data.source_content_hash, "来源指纹", CODE_MAX_LEN)?;
        let erp_content_hash = normalize_optional_text(data.erp_content_hash, "ERP 指纹", CODE_MAX_LEN)?;
        Self::ensure_type_consistency(data.difference_type, data.sales_order_id.as_ref())?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            reconciliation_job_id: data.reconciliation_job_id,
            external_order_key: ExternalOrderKey::from_trimmed(&external_order_no),
            external_order_no,
            source_updated_at: data.source_updated_at,
            source_status_code,
            source_content_hash,
            sales_order_id: data.sales_order_id,
            erp_revision_id: data.erp_revision_id,
            erp_content_hash,
            difference_type: data.difference_type,
            status: ReconciliationItemStatus::Pending,
            single_order_sync_job_id: None,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        })
    }

    /// 发起按单号补拉。
    ///
    /// 按原来源身份发起单号补拉（§6.13），同一时间只允许一个补拉任务。
    ///
    /// # 参数
    /// * `sync_job_id` - 按单号补拉任务
    ///
    /// # 返回
    /// 发起成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 明细已离开待处理状态时返回错误。
    pub fn start_backfill(&mut self, sync_job_id: MallSalesSyncJobId) -> Result<()> {
        ensure_transition(self.status, ReconciliationItemStatus::Backfilling)?;
        self.status = ReconciliationItemStatus::Backfilling;
        self.single_order_sync_job_id = Some(sync_job_id);
        Ok(())
    }

    /// 登记人工解决。
    ///
    /// 差异解决后使用原快照和原幂等身份重新归集，不手工补建另一张销售单
    /// （§6.13）。
    ///
    /// # 参数
    /// * `resolution` - 人工处理结论
    /// * `resolved_by` - 处理人
    /// * `resolved_at` - 处理时间
    ///
    /// # 返回
    /// 解决成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 明细已离开可解决状态，或处理结论为空/超长时返回错误。
    pub fn resolve(&mut self, resolution: String, resolved_by: String, resolved_at: Instant) -> Result<()> {
        ensure_transition(self.status, ReconciliationItemStatus::Resolved)?;
        self.status = ReconciliationItemStatus::Resolved;
        self.resolution = Some(normalize_required_text(
            resolution,
            "处理结论不能为空",
            RESOLUTION_MAX_LEN,
            "处理结论过长",
        )?);
        self.resolved_by = Some(normalize_required_text(
            resolved_by,
            "处理人不能为空",
            RESOLVER_MAX_LEN,
            "处理人过长",
        )?);
        self.resolved_at = Some(resolved_at);
        Ok(())
    }

    /// 登记确认无误。
    ///
    /// 补拉后按原来源身份重新比对一致（§6.13），不要求处理结论。
    ///
    /// # 参数
    /// * `resolved_by` - 确认人
    /// * `resolved_at` - 确认时间
    ///
    /// # 返回
    /// 确认成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 明细已离开可确认状态，或确认人为空时返回错误。
    pub fn confirm_no_difference(&mut self, resolved_by: String, resolved_at: Instant) -> Result<()> {
        ensure_transition(self.status, ReconciliationItemStatus::ConfirmedNoDifference)?;
        self.status = ReconciliationItemStatus::ConfirmedNoDifference;
        self.resolved_by = Some(normalize_required_text(
            resolved_by,
            "确认人不能为空",
            RESOLVER_MAX_LEN,
            "确认人过长",
        )?);
        self.resolved_at = Some(resolved_at);
        Ok(())
    }

    /// 校验差异类型与 ERP 侧存在性的一致性。
    ///
    /// # 参数
    /// * `difference_type` - 差异类型
    /// * `sales_order_id` - ERP 销售单 ID
    ///
    /// # 错误
    /// `ERP 缺失` 携带销售单 ID，或双侧比对类型缺少销售单 ID 时返回错误。
    fn ensure_type_consistency(
        difference_type: ReconciliationDifferenceType,
        sales_order_id: Option<&SalesOrderId>,
    ) -> Result<()> {
        let both_sides = !matches!(
            difference_type,
            ReconciliationDifferenceType::MallMissing | ReconciliationDifferenceType::ErpMissing
        );
        if difference_type == ReconciliationDifferenceType::ErpMissing && sales_order_id.is_some() {
            return Err(Error::from("ERP 缺失差异不得携带 ERP 销售单 ID"));
        }
        if both_sides && sales_order_id.is_none() {
            return Err(Error::from("双侧比对差异必须携带 ERP 销售单 ID"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::ids::{MallSalesReconciliationItemId, MallSalesReconciliationJobId};

    fn job_data() -> MallSalesReconciliationJobData {
        MallSalesReconciliationJobData {
            source_system_id: SourceSystemId::new("sys-mall"),
            job_no: " REC-2026-06 ".to_string(),
            source_list_as_of: Instant::from_unix_secs(1_700_000_000),
            started_at: Instant::from_unix_secs(1_700_000_100),
        }
    }

    fn item_data() -> MallSalesReconciliationItemData {
        MallSalesReconciliationItemData {
            reconciliation_job_id: MallSalesReconciliationJobId::new("job-1"),
            external_order_no: " SO-1 ".to_string(),
            source_status_code: " EFFECTIVE ".to_string(),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            source_content_hash: Some(" h1 ".to_string()),
            sales_order_id: Some(SalesOrderId::new("so-1")),
            erp_revision_id: Some(SalesOrderRevisionId::new("rev-1")),
            erp_content_hash: Some(" h2 ".to_string()),
            difference_type: ReconciliationDifferenceType::StatusDifference,
        }
    }

    #[test]
    fn job_new_trims_job_no_and_starts_running() {
        let job =
            MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-1"), job_data()).unwrap();

        assert_eq!(job.job_no, "REC-2026-06");
        assert_eq!(job.status, ReconciliationJobStatus::Running);
        assert_eq!((job.source_count, job.erp_count, job.difference_count), (0, 0, 0));
        assert!(job.finished_at.is_none());
    }

    #[test]
    fn job_new_rejects_empty_and_overlong_job_no() {
        let empty_no = MallSalesReconciliationJobData {
            job_no: "   ".to_string(),
            ..job_data()
        };
        assert!(MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-2"), empty_no).is_err());

        let overlong_no = MallSalesReconciliationJobData {
            job_no: "x".repeat(JOB_NO_MAX_LEN + 1),
            ..job_data()
        };
        assert!(
            MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-3"), overlong_no).is_err()
        );
    }

    #[test]
    fn job_finish_must_match_difference_counts() {
        let mut job =
            MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-4"), job_data()).unwrap();
        job.record_counts(100, 98, 2).unwrap();
        assert_eq!(job.source_count, 100);

        assert!(
            job.finish(
                ReconciliationJobStatus::Completed,
                Instant::from_unix_secs(1_700_000_200)
            )
            .is_err(),
            "有差异不能记为完成"
        );
        job.finish(
            ReconciliationJobStatus::HasDifference,
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
        assert!(job.finished_at.is_some());

        let mut clean =
            MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-5"), job_data()).unwrap();
        clean.record_counts(100, 100, 0).unwrap();
        clean
            .finish(
                ReconciliationJobStatus::Completed,
                Instant::from_unix_secs(1_700_000_200),
            )
            .unwrap();
        assert!(
            clean
                .finish(
                    ReconciliationJobStatus::Failed,
                    Instant::from_unix_secs(1_700_000_300)
                )
                .is_err(),
            "终态不可回退"
        );
    }

    #[test]
    fn job_record_counts_rejects_impossible_difference() {
        let mut job =
            MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-6"), job_data()).unwrap();
        assert!(job.record_counts(100, 100, 201).is_err());
        assert!(job.record_counts(100, 100, 200).is_ok());
    }

    #[test]
    fn item_new_trims_and_computes_key() {
        let item =
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-1"), item_data()).unwrap();

        assert_eq!(item.external_order_no, "SO-1");
        assert_eq!(item.external_order_key.as_bytes(), b"SO-1");
        assert_eq!(item.source_status_code, "EFFECTIVE");
        assert_eq!(item.source_content_hash.as_deref(), Some("h1"));
        assert_eq!(item.status, ReconciliationItemStatus::Pending);
        assert!(item.resolution.is_none());
    }

    #[test]
    fn item_new_enforces_difference_type_consistency() {
        let erp_missing_with_order = MallSalesReconciliationItemData {
            difference_type: ReconciliationDifferenceType::ErpMissing,
            sales_order_id: Some(SalesOrderId::new("so-1")),
            ..item_data()
        };
        assert!(
            MallSalesReconciliationItem::new(
                MallSalesReconciliationItemId::new("i-2"),
                erp_missing_with_order
            )
            .is_err(),
            "ERP 缺失不得携带销售单 ID"
        );

        let status_without_order = MallSalesReconciliationItemData {
            difference_type: ReconciliationDifferenceType::StatusDifference,
            sales_order_id: None,
            ..item_data()
        };
        assert!(
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-3"), status_without_order)
                .is_err(),
            "双侧比对必须携带销售单 ID"
        );

        let mall_missing = MallSalesReconciliationItemData {
            difference_type: ReconciliationDifferenceType::MallMissing,
            sales_order_id: Some(SalesOrderId::new("so-1")),
            ..item_data()
        };
        assert!(
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-4"), mall_missing).is_ok()
        );
    }

    #[test]
    fn item_new_rejects_empty_and_overlong_fields() {
        let empty_no = MallSalesReconciliationItemData {
            external_order_no: "   ".to_string(),
            ..item_data()
        };
        assert!(
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-5"), empty_no).is_err()
        );

        let overlong_status = MallSalesReconciliationItemData {
            source_status_code: "x".repeat(CODE_MAX_LEN + 1),
            ..item_data()
        };
        assert!(
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-6"), overlong_status)
                .is_err()
        );
    }

    #[test]
    fn item_lifecycle_backfill_then_resolve_or_confirm() {
        let mut item =
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-7"), item_data()).unwrap();

        item.start_backfill(MallSalesSyncJobId::new("bf-1")).unwrap();
        assert_eq!(item.status, ReconciliationItemStatus::Backfilling);
        assert_eq!(
            item.single_order_sync_job_id,
            Some(MallSalesSyncJobId::new("bf-1"))
        );

        item.resolve(
            " 补拉后一致，保留 ERP 版本 ".to_string(),
            " 财务-李四 ".to_string(),
            Instant::from_unix_secs(1_700_000_300),
        )
        .unwrap();
        assert_eq!(item.status, ReconciliationItemStatus::Resolved);
        assert_eq!(item.resolution.as_deref(), Some("补拉后一致，保留 ERP 版本"));

        let mut direct =
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-8"), item_data()).unwrap();
        direct
            .confirm_no_difference("系统".to_string(), Instant::from_unix_secs(1_700_000_300))
            .unwrap();
        assert_eq!(direct.status, ReconciliationItemStatus::ConfirmedNoDifference);
        assert!(direct.resolution.is_none());
    }

    #[test]
    fn item_resolve_requires_resolution_text() {
        let mut item =
            MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-9"), item_data()).unwrap();
        assert!(item
            .resolve(
                String::new(),
                "财务".to_string(),
                Instant::from_unix_secs(1_700_000_300)
            )
            .is_err());
        assert!(item
            .resolve(
                "结论".to_string(),
                "   ".to_string(),
                Instant::from_unix_secs(1_700_000_300)
            )
            .is_err());
    }

    #[test]
    fn item_status_machine_is_directed() {
        assert!(ensure_transition(
            ReconciliationItemStatus::Pending,
            ReconciliationItemStatus::Backfilling
        )
        .is_ok());
        assert!(ensure_transition(
            ReconciliationItemStatus::Pending,
            ReconciliationItemStatus::Resolved
        )
        .is_ok());
        assert!(ensure_transition(
            ReconciliationItemStatus::Backfilling,
            ReconciliationItemStatus::ConfirmedNoDifference
        )
        .is_ok());
        assert!(ensure_transition(
            ReconciliationItemStatus::Backfilling,
            ReconciliationItemStatus::Pending
        )
        .is_err());
        assert!(ensure_transition(
            ReconciliationItemStatus::Resolved,
            ReconciliationItemStatus::Backfilling
        )
        .is_err());
    }

    #[test]
    fn status_and_type_serde_use_stable_codes() {
        assert_eq!(
            serde_json::to_string(&ReconciliationJobStatus::HasDifference).unwrap(),
            "\"has_difference\""
        );
        assert_eq!(
            serde_json::to_string(&ReconciliationDifferenceType::ContentFingerprintDifference).unwrap(),
            "\"content_fingerprint_difference\""
        );
        assert_eq!(
            serde_json::to_string(&ReconciliationItemStatus::ConfirmedNoDifference).unwrap(),
            "\"confirmed_no_difference\""
        );
        assert_eq!(ReconciliationDifferenceType::ErpMissing.label(), "ERP 缺失");
        assert_eq!(ReconciliationItemStatus::Backfilling.label(), "补拉中");
    }

    #[test]
    fn bson_roundtrip_preserves_job_and_item() {
        let job =
            MallSalesReconciliationJob::new(MallSalesReconciliationJobId::new("j-7"), job_data()).unwrap();
        let job_back: MallSalesReconciliationJob =
            bson::deserialize_from_document(bson::serialize_to_document(&job).unwrap()).unwrap();
        assert_eq!(job_back, job);

        let item = MallSalesReconciliationItem::new(MallSalesReconciliationItemId::new("i-10"), item_data())
            .unwrap();
        let item_back: MallSalesReconciliationItem =
            bson::deserialize_from_document(bson::serialize_to_document(&item).unwrap()).unwrap();
        assert_eq!(item_back, item);
    }
}
