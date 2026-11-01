//! 申请状态和历史不变式；正式供给结果与提交快照分别保存。
use application_core::AuditActor;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::{ApplicationKind, OfferingApplicationSnapshot};
use crate::{Error, Result};

/// 采购单人确认的申请生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApplicationStatus {
    Draft,
    Submitted,
    Returned,
    Withdrawn,
    Effective,
}
/// 每次提交的不可变供应商原稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenOfferingSubmission {
    pub submission_no: u32,
    pub submitted_by: String,
    pub submitted_at: Instant,
    pub snapshot: OfferingApplicationSnapshot,
    pub reason: String,
    pub handler_id: String,
    pub work_item_id: String,
    pub work_item_version: u64,
}
/// 供应商可见的历次采购决定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationDecision {
    pub submission_no: u32,
    pub decided_by: String,
    pub decided_at: Instant,
    pub status: ApplicationStatus,
    pub reason: String,
}
/// 已提交的正式供给结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfferingApplicationResult {
    pub offering_id: String,
    pub revision_id: String,
    pub revision_no: u32,
    pub offering_version: u64,
    pub operation: String,
}
/// 供给商业申请；仅待确认状态可以作采购决定。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct OfferingApplication {
    #[serde(flatten)]
    pub base: BaseModel,
    pub supplier_id: String,
    pub created_by: String,
    pub kind: ApplicationKind,
    pub status: ApplicationStatus,
    pub snapshot: OfferingApplicationSnapshot,
    pub reason: String,
    pub submissions: Vec<FrozenOfferingSubmission>,
    pub decisions: Vec<ApplicationDecision>,
    pub result: Option<OfferingApplicationResult>,
}
impl OfferingApplication {
    /// 核验持久化前历史前缀和稳定身份没有改写。
    /// # 参数
    /// `before` 是同Executor读取的现存申请。
    /// # 返回
    /// 新状态只追加合法历史时成功。
    /// # 错误
    /// 历史删除、原稿改写、稳定身份变化或版本冲突拒绝。
    pub fn ensure_history_preserved(&self, before: &Self) -> Result<()> {
        if self.base.id != before.base.id
            || self.supplier_id != before.supplier_id
            || self.created_by != before.created_by
            || self.base.version != before.base.version
        {
            return Err(Error::ConflictError("申请身份或版本已变化".into()));
        }
        if self.submissions.len() < before.submissions.len() || self.decisions.len() < before.decisions.len()
        {
            return Err(Error::ConflictError("申请提交及决定历史不得删除".into()));
        }
        let submissions = serde_json::to_value(&self.submissions[..before.submissions.len()])
            .map_err(|error| Error::Internal(error.to_string()))?;
        let previous =
            serde_json::to_value(&before.submissions).map_err(|error| Error::Internal(error.to_string()))?;
        let decisions = serde_json::to_value(&self.decisions[..before.decisions.len()])
            .map_err(|error| Error::Internal(error.to_string()))?;
        let previous_decisions =
            serde_json::to_value(&before.decisions).map_err(|error| Error::Internal(error.to_string()))?;
        if submissions != previous || decisions != previous_decisions {
            return Err(Error::ConflictError("申请提交及决定历史不得改写".into()));
        }
        if before.status == ApplicationStatus::Effective {
            return Err(Error::ConflictError("已生效申请不得再次修改".into()));
        }
        if before.status == ApplicationStatus::Submitted
            && serde_json::to_value(&self.snapshot).map_err(|error| Error::Internal(error.to_string()))?
                != serde_json::to_value(&before.snapshot)
                    .map_err(|error| Error::Internal(error.to_string()))?
        {
            return Err(Error::ConflictError("待确认申请原稿不得改写".into()));
        }
        Ok(())
    }
    /// 创建供应商草稿。
    /// # 参数
    /// `id` 为服务器主键；`supplier_id` 来自当前有效绑定。
    /// # 返回
    /// 无正式结果的草稿。
    /// # 错误
    /// 外部身份或必填字段无效时拒绝。
    pub fn new(
        id: String,
        supplier_id: &str,
        actor: &AuditActor,
        snapshot: OfferingApplicationSnapshot,
        reason: &str,
    ) -> Result<Self> {
        ensure_supplier(actor, supplier_id)?;
        ensure_text(&id, "申请标识")?;
        ensure_text(reason, "申请原因")?;
        Ok(Self {
            base: BaseModel::new(id),
            supplier_id: supplier_id.to_string(),
            created_by: actor.id().to_string(),
            kind: snapshot.kind(),
            status: ApplicationStatus::Draft,
            snapshot,
            reason: reason.trim().to_string(),
            submissions: Vec::new(),
            decisions: Vec::new(),
            result: None,
        })
    }
    /// 检查版本及供应商归属。
    /// # 参数
    /// `expected` 是客户端读取版本；绑定由服务器注入。
    /// # 返回
    /// 当前申请可以访问时成功。
    /// # 错误
    /// 范围外统一不存在；过期版本返回冲突。
    pub fn ensure_owned(&self, supplier_id: &str, actor: &AuditActor, expected: u64) -> Result<()> {
        ensure_supplier(actor, supplier_id)?;
        if self.supplier_id != supplier_id || self.base.is_deleted() {
            return Err(Error::NotFound("申请不存在或无权查看".into()));
        }
        if self.base.version != expected {
            return Err(Error::ConflictError("申请版本已变化，请重新核对".into()));
        }
        Ok(())
    }
    /// 修改未提交草稿，保留此前冻结历史。
    /// # 参数
    /// `snapshot` 是重新核对后的供应商原稿。
    /// # 返回
    /// 更新当前可编辑稿。
    /// # 错误
    /// 待确认和已生效状态不可编辑。
    pub fn edit(&mut self, snapshot: OfferingApplicationSnapshot, reason: &str) -> Result<()> {
        if !matches!(
            self.status,
            ApplicationStatus::Draft | ApplicationStatus::Returned | ApplicationStatus::Withdrawn
        ) {
            return Err(Error::ConflictError("待确认或已生效申请不可修改".into()));
        }
        ensure_text(reason, "申请原因")?;
        if snapshot.kind() != self.kind {
            return Err(Error::ValidationError("申请类型不可修改".into()));
        }
        self.snapshot = snapshot;
        self.reason = reason.trim().to_string();
        self.status = ApplicationStatus::Draft;
        Ok(())
    }
    /// 冻结原稿并登记具体确认任务。
    /// # 参数
    /// 当前供应商操作人、处理人及新任务由组合层校验。
    /// # 返回
    /// 进入待采购确认。
    /// # 错误
    /// 待确认/已生效、处理人或任务缺失时拒绝。
    pub fn submit(
        &mut self,
        actor: &AuditActor,
        handler_id: &str,
        work_item_id: &str,
        work_item_version: u64,
        at: Instant,
    ) -> Result<()> {
        ensure_supplier(actor, &self.supplier_id)?;
        if !matches!(
            self.status,
            ApplicationStatus::Draft | ApplicationStatus::Returned | ApplicationStatus::Withdrawn
        ) {
            return Err(Error::ConflictError("申请不可重复提交".into()));
        }
        ensure_text(handler_id, "确认处理人")?;
        ensure_text(work_item_id, "确认任务")?;
        if work_item_version == 0 {
            return Err(Error::ValidationError("确认任务版本无效".into()));
        }
        let submission_no = u32::try_from(self.submissions.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Error::ConflictError("提交次数超限".into()))?;
        self.submissions.push(FrozenOfferingSubmission {
            submission_no,
            submitted_by: actor.id().to_string(),
            submitted_at: at,
            snapshot: self.snapshot.clone(),
            reason: self.reason.clone(),
            handler_id: handler_id.to_string(),
            work_item_id: work_item_id.to_string(),
            work_item_version,
        });
        self.status = ApplicationStatus::Submitted;
        Ok(())
    }
    /// 撤回未完成申请，保留冻结历史。
    /// # 参数
    /// `actor` 是有效绑定的供应商身份。
    /// # 返回
    /// 进入已撤回。
    /// # 错误
    /// 当前状态不是 `Submitted` 时返回 `ConflictError`；操作人不是供应商门户身份时返回 `Forbidden`；供应商绑定为空或超长时返回 `ValidationError`。
    pub fn withdraw(&mut self, actor: &AuditActor) -> Result<()> {
        ensure_supplier(actor, &self.supplier_id)?;
        if self.status != ApplicationStatus::Submitted {
            return Err(Error::ConflictError("申请已处理，不能撤回".into()));
        }
        self.status = ApplicationStatus::Withdrawn;
        Ok(())
    }
    /// 检查当前任务及申请仍可确认。
    /// # 参数
    /// 内部处理人、读取时申请版本和具体任务版本。
    /// # 返回
    /// 对应的冻结提交。
    /// # 错误
    /// 操作人不是 `AccountKind::Admin` 时返回 `Forbidden`；状态不是 `Submitted`、版本不一致、任务标识不符或任务版本为 0 时返回 `ConflictError`；没有冻结提交时返回 `Internal`。
    pub fn decision_submission(
        &self,
        actor: &AuditActor,
        expected_version: u64,
        task_id: &str,
        task_version: u64,
    ) -> Result<&FrozenOfferingSubmission> {
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("仅内部人员可确认申请".into()));
        }
        if self.status != ApplicationStatus::Submitted || self.base.version != expected_version {
            return Err(Error::ConflictError("申请状态或版本已变化".into()));
        }
        let submission = self.submissions.last().ok_or_else(|| Error::Internal("申请缺少冻结提交".into()))?;
        if submission.work_item_id != task_id || task_version == 0 {
            return Err(Error::ConflictError("申请任务身份无效".into()));
        }
        Ok(submission)
    }
    /// 保存退回或正式生效决定。
    /// # 参数
    /// `result` 仅在正式业务事实同事务完成后传入。
    /// # 返回
    /// 更新当前结果，追加不可变决定历史。
    /// # 错误
    /// 缺少冻结记录、退回原因或重复处理时拒绝。
    pub fn decide(
        &mut self,
        actor: &AuditActor,
        result: Option<OfferingApplicationResult>,
        reason: &str,
        at: Instant,
    ) -> Result<()> {
        if self.status != ApplicationStatus::Submitted {
            return Err(Error::ConflictError("申请已处理".into()));
        }
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("仅内部人员可确认申请".into()));
        }
        ensure_text(reason, "处理原因")?;
        let submission = self.submissions.last().ok_or_else(|| Error::Internal("申请缺少冻结提交".into()))?;
        self.status =
            if result.is_some() { ApplicationStatus::Effective } else { ApplicationStatus::Returned };
        self.decisions.push(ApplicationDecision {
            submission_no: submission.submission_no,
            decided_by: actor.id().to_string(),
            decided_at: at,
            status: self.status,
            reason: reason.trim().to_string(),
        });
        self.result = result;
        Ok(())
    }
}
/// 指定 SKU 对指定供应商的报价资格，不授予商品维护权。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct QuoteAccessGrant {
    #[serde(flatten)]
    pub base: BaseModel,
    pub supplier_id: String,
    pub sku_id: String,
    pub active: bool,
    pub granted_by: String,
    pub opened_by: String,
    pub opened_at: Instant,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<Instant>,
    pub history: Vec<QuoteAccessDecision>,
}
/// 定向报价资格的不可变变更记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteAccessDecision {
    pub active: bool,
    pub actor_id: String,
    pub at: Instant,
}
impl QuoteAccessGrant {
    /// 创建明确归属的定向开放记录。
    /// # 参数
    /// 供应商、SKU 和内部操作人。
    /// # 返回
    /// 启用的开放记录。
    /// # 错误
    /// 空值或外部操作人拒绝。
    pub fn new(id: String, supplier_id: &str, sku_id: &str, actor: &AuditActor) -> Result<Self> {
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("仅内部人员可开放报价".into()));
        }
        ensure_text(supplier_id, "供应商")?;
        ensure_text(sku_id, "公司SKU")?;
        let opened_at = Instant::now();
        Ok(Self {
            base: BaseModel::new(id),
            supplier_id: supplier_id.to_string(),
            sku_id: sku_id.to_string(),
            active: true,
            granted_by: actor.id().to_string(),
            opened_by: actor.id().to_string(),
            opened_at,
            revoked_by: None,
            revoked_at: None,
            history: vec![QuoteAccessDecision {
                active: true,
                actor_id: actor.id().to_string(),
                at: opened_at,
            }],
        })
    }
    /// 追加资格变更并保留原开放事实。
    /// # 参数
    /// 新状态、真实内部操作人及处理时间。
    /// # 返回
    /// 状态有变化时更新当前状态并追加不可变历史；状态未变时不改记录并成功。
    /// # 错误
    /// 外部门户身份不允许变更资格。
    pub fn set_active(&mut self, active: bool, actor: &AuditActor, at: Instant) -> Result<()> {
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("仅内部人员可变更报价开放".into()));
        }
        if self.active == active {
            return Ok(());
        }
        self.active = active;
        if !active {
            self.revoked_by = Some(actor.id().to_string());
            self.revoked_at = Some(at);
        }
        self.history.push(QuoteAccessDecision { active, actor_id: actor.id().to_string(), at });
        Ok(())
    }
}
/// 确认操作人是该供应商的门户身份。
///
/// # 参数
/// * `actor` - 当前操作人。
/// * `supplier_id` - 服务器注入的供应商绑定。
///
/// # 返回
/// 身份种类与绑定均有效时成功。
///
/// # 错误
/// `actor` 不是 `AccountKind::Supplier` 时返回 `Forbidden`；`supplier_id` 为空白或超过 1024 字节时返回 `ValidationError`。
pub(super) fn ensure_supplier(actor: &AuditActor, supplier_id: &str) -> Result<()> {
    if actor.kind() != AccountKind::Supplier {
        return Err(Error::Forbidden("供应商门户身份无效".into()));
    }
    ensure_text(supplier_id, "供应商绑定")
}
/// 拒绝空白或超过 1024 字节的文本。
///
/// # 参数
/// * `value` - 待检查文本，长度按原始字节计算。
/// * `name` - 写入校验错误的字段名。
///
/// # 返回
/// 含非空白字符且字节长度不超过 1024 时成功。
///
/// # 错误
/// 空白或超长时返回 `ValidationError`，文案为 `{name}为空或超出长度`。
pub(super) fn ensure_text(value: &str, name: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 1024 {
        return Err(Error::ValidationError(format!("{name}为空或超出长度")));
    }
    Ok(())
}
