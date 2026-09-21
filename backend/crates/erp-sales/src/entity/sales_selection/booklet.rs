//! 选品册聚合根。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::state::ensure_transition;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId, SalesSelectionProposalId};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::limits::LINK_TTL_DAYS;
use super::pool::PoolSource;
use super::status::BookletStatus;
use super::tier::{TierRule, normalize_tiers};
use super::types::{PrepareKind, SelectionForm, SubmitMode};

/// 选品册创建数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesSelectionBookletData {
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 客户编号快照。
    pub customer_no: String,
    /// 客户展示名称快照。
    pub customer_name: String,
    /// 显式销售负责人；册建立时必填，客户提交人不成为负责人。
    pub sales_owner_user_id: String,
    /// 业务组织；册建立时必填，取负责人有效主属组织。
    pub business_org_unit_id: String,
    /// 选品形态。
    pub form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 商品池来源。
    pub pool_source: PoolSource,
    /// 套餐档位；单品必须为空。
    pub tiers: Vec<TierRule>,
    /// 创建人。
    pub created_by: String,
}

/// 选品册。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionBooklet {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户稳定身份，创建后不可改绑。
    pub customer_id: CustomerAccountId,
    /// 客户编号快照。
    pub customer_no: String,
    /// 客户展示名称快照。
    pub customer_name: String,
    /// 显式销售负责人；创建后不随提交人或编辑人变化，改派走 S3-07。
    /// 组织范围落地前的历史文档可能缺此字段，读取时默认为空并由准备队列跳过。
    #[serde(default)]
    pub sales_owner_user_id: String,
    /// 业务组织；当前单据团队口径，调岗不自动改写。
    /// 组织范围落地前的历史文档可能缺此字段，读取时默认为空并由准备队列跳过。
    #[serde(default)]
    pub business_org_unit_id: String,
    /// 选品形态。
    pub form: SelectionForm,
    /// 提交方式。
    pub submit_mode: SubmitMode,
    /// 商品池来源。
    pub pool_source: PoolSource,
    /// 档位规则。
    pub tiers: Vec<TierRule>,
    /// 业务状态。
    pub status: BookletStatus,
    /// 当前有效准备批次。
    pub current_batch_id: Option<String>,
    /// 活动准备任务。
    pub active_task_id: Option<String>,
    /// 进入准备中之前的状态，用于失败恢复。
    pub pre_prepare_status: Option<BookletStatus>,
    /// 资格业务日期。
    pub eligibility_as_of: Option<BusinessDate>,
    /// 准备完成时间。
    pub prepared_at: Option<Instant>,
    /// 最近一次准备失败原因。
    pub last_prepare_failure: Option<String>,
    /// 当前令牌哈希。
    pub link_token_hash: Option<String>,
    /// 当前令牌密文。
    pub link_token_ciphertext: Option<String>,
    /// 令牌版本，更换链接时递增。
    pub link_token_version: u32,
    /// 链接到期时间。
    pub link_expires_at: Option<Instant>,
    /// 是否已撤销公开访问。
    pub link_revoked: bool,
    /// 关联销售方案。
    pub proposal_id: Option<SalesSelectionProposalId>,
    /// 创建人。
    pub created_by: String,
    /// 更新人。
    pub updated_by: String,
    /// 发布人。
    pub published_by: Option<String>,
    /// 发布时间。
    pub published_at: Option<Instant>,
    /// 关闭时间。
    pub closed_at: Option<Instant>,
    /// 提交时间。
    pub submitted_at: Option<Instant>,
    /// 作废时间。
    pub voided_at: Option<Instant>,
}

impl SalesSelectionBooklet {
    /// 创建草稿选品册。
    ///
    /// # 参数
    /// * `id` - 册身份
    /// * `data` - 创建字段
    ///
    /// # 返回
    /// 返回草稿状态的选品册。
    ///
    /// # 错误
    /// 缺少客户、形态、提交方式或档位不合规时拒绝。
    pub fn new(id: SalesSelectionBookletId, data: SalesSelectionBookletData) -> Result<Self> {
        let created = normalize_actor(&data.created_by, "创建人不能为空")?;
        let customer_no = normalize_required_text(data.customer_no, "客户编号不能为空", 64, "客户编号过长")?;
        let customer_name =
            normalize_required_text(data.customer_name, "客户名称不能为空", 128, "客户名称过长")?;
        let owner =
            normalize_required_text(data.sales_owner_user_id, "销售负责人不能为空", 64, "销售负责人过长")?;
        let org = normalize_required_text(data.business_org_unit_id, "业务组织不能为空", 64, "业务组织过长")?;
        let tiers = normalize_form_tiers(data.form, data.tiers)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            customer_id: data.customer_id,
            customer_no,
            customer_name,
            sales_owner_user_id: owner,
            business_org_unit_id: org,
            form: data.form,
            submit_mode: data.submit_mode,
            pool_source: data.pool_source,
            tiers,
            status: BookletStatus::Draft,
            current_batch_id: None,
            active_task_id: None,
            pre_prepare_status: None,
            eligibility_as_of: None,
            prepared_at: None,
            last_prepare_failure: None,
            link_token_hash: None,
            link_token_ciphertext: None,
            link_token_version: 0,
            link_expires_at: None,
            link_revoked: false,
            proposal_id: None,
            created_by: created.clone(),
            updated_by: created,
            published_by: None,
            published_at: None,
            closed_at: None,
            submitted_at: None,
            voided_at: None,
        })
    }

    /// 草稿内更新筛选、勾选或档位规则，不改变来源类型。
    ///
    /// # 参数
    /// * `pool_source` - 同源类型的新来源
    /// * `tiers` - 新档位；单品必须为空
    /// * `actor_id` - 更新人
    ///
    /// # 返回
    /// 成功时更新规则。
    ///
    /// # 错误
    /// 非草稿、来源类型变化或档位非法时拒绝。
    pub fn update_draft_rules(
        &mut self,
        pool_source: PoolSource,
        tiers: Vec<TierRule>,
        actor_id: &str,
    ) -> Result<()> {
        if !self.status.allows_draft_edit() {
            return Err(Error::from("只有草稿可以修改商品池或档位规则"));
        }
        if pool_source.kind != self.pool_source.kind {
            return Err(Error::from("商品池来源类型创建后不可修改"));
        }
        self.tiers = normalize_form_tiers(self.form, tiers)?;
        self.pool_source = pool_source;
        self.touch(actor_id)?;
        Ok(())
    }

    /// 整册重新准备时在内存中替换筛选、勾选或档位，不改变来源类型。
    ///
    /// 仅在准备成功落库后成为有效规则；失败路径必须先恢复原规则再 `fail_prepare`。
    ///
    /// # 参数
    /// * `pool_source` - 同源类型的新来源
    /// * `tiers` - 新档位；单品必须为空
    ///
    /// # 返回
    /// 成功时更新内存中的规则。
    ///
    /// # 错误
    /// 来源类型变化或档位非法时拒绝。
    pub fn apply_reprepare_rules(&mut self, pool_source: PoolSource, tiers: Vec<TierRule>) -> Result<()> {
        if pool_source.kind != self.pool_source.kind {
            return Err(Error::from("商品池来源类型创建后不可修改"));
        }
        self.tiers = normalize_form_tiers(self.form, tiers)?;
        self.pool_source = pool_source;
        Ok(())
    }

    /// 进入准备中并占用唯一活动任务。
    ///
    /// # 参数
    /// * `task_id` - 新任务身份
    /// * `kind` - 准备种类
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 册进入准备中。
    ///
    /// # 错误
    /// 状态不允许、已有活动任务或单品请求组合重生成时拒绝。
    pub fn begin_prepare(&mut self, task_id: &str, kind: PrepareKind, actor_id: &str) -> Result<()> {
        self.ensure_no_active_task()?;
        if !self.status.allows_prepare() {
            return Err(Error::from("当前状态不能开始准备"));
        }
        if !self.form.is_package() && kind.reuses_current_batch() {
            return Err(Error::from("单品不支持组合重生成，请整册重新准备"));
        }
        if matches!(self.status, BookletStatus::Draft) && kind != PrepareKind::FirstPrepare {
            return Err(Error::from("草稿只能发起首次准备"));
        }
        if matches!(self.status, BookletStatus::PendingPublish) && kind == PrepareKind::FirstPrepare {
            return Err(Error::from("待发布请使用重生成或整册重新准备"));
        }
        ensure_transition(self.status, BookletStatus::Preparing)?;
        self.pre_prepare_status = Some(self.status);
        self.status = BookletStatus::Preparing;
        self.active_task_id = Some(task_id.to_string());
        self.last_prepare_failure = None;
        self.touch(actor_id)
    }

    /// 准备成功，进入待发布。
    ///
    /// # 参数
    /// * `batch_id` - 新有效批次
    /// * `eligibility_as_of` - 资格业务日期
    /// * `prepared_at` - 准备时间
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 册进入待发布并清除活动任务。
    ///
    /// # 错误
    /// 当前不是准备中时拒绝。
    pub fn complete_prepare(
        &mut self,
        batch_id: &str,
        eligibility_as_of: BusinessDate,
        prepared_at: Instant,
        actor_id: &str,
    ) -> Result<()> {
        ensure_transition(self.status, BookletStatus::PendingPublish)?;
        self.status = BookletStatus::PendingPublish;
        self.current_batch_id = Some(batch_id.to_string());
        self.eligibility_as_of = Some(eligibility_as_of);
        self.prepared_at = Some(prepared_at);
        self.clear_task();
        self.touch(actor_id)
    }

    /// 准备失败并按规则恢复。
    ///
    /// # 参数
    /// * `reason` - 失败原因
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 首次准备回草稿；从待发布发起则恢复待发布。
    ///
    /// # 错误
    /// 当前不是准备中时拒绝。
    pub fn fail_prepare(&mut self, reason: &str, actor_id: &str) -> Result<()> {
        let restore = self.pre_prepare_status.unwrap_or(BookletStatus::Draft);
        ensure_transition(self.status, restore)?;
        self.status = restore;
        self.last_prepare_failure = Some(reason.trim().to_string());
        self.clear_task();
        self.touch(actor_id)
    }

    /// 发布当前批次并写入链接。
    ///
    /// # 参数
    /// * `token_hash` - 令牌哈希
    /// * `token_ciphertext` - 令牌密文
    /// * `published_at` - 发布时间
    /// * `actor_id` - 发布人
    ///
    /// # 返回
    /// 册进入已发布，有效期为 30 天。
    ///
    /// # 错误
    /// 非待发布或缺少有效批次时拒绝。
    pub fn publish(
        &mut self,
        token_hash: String,
        token_ciphertext: String,
        published_at: Instant,
        actor_id: &str,
    ) -> Result<()> {
        if !self.status.allows_publish() {
            return Err(Error::from("只有待发布的选品册可以发布"));
        }
        if self.current_batch_id.is_none() {
            return Err(Error::from("没有可发布的准备批次"));
        }
        ensure_transition(self.status, BookletStatus::Published)?;
        self.status = BookletStatus::Published;
        self.link_token_hash = Some(token_hash);
        self.link_token_ciphertext = Some(token_ciphertext);
        self.link_token_version = self.link_token_version.saturating_add(1);
        self.link_expires_at = Some(add_days(published_at, LINK_TTL_DAYS));
        self.link_revoked = false;
        self.published_by = Some(normalize_actor(actor_id, "发布人不能为空")?);
        self.published_at = Some(published_at);
        self.touch(actor_id)
    }

    /// 更换链接：原令牌立即失效，到期时间不变。
    ///
    /// # 参数
    /// * `token_hash` - 新哈希
    /// * `token_ciphertext` - 新密文
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 令牌版本递增。
    ///
    /// # 错误
    /// 非已发布状态时拒绝。
    pub fn rotate_link(
        &mut self,
        token_hash: String,
        token_ciphertext: String,
        actor_id: &str,
    ) -> Result<()> {
        if self.status != BookletStatus::Published {
            return Err(Error::from("只有已发布且未提交的选品册可以更换链接"));
        }
        self.link_token_hash = Some(token_hash);
        self.link_token_ciphertext = Some(token_ciphertext);
        self.link_token_version = self.link_token_version.saturating_add(1);
        self.touch(actor_id)
    }

    /// 关闭未提交选品册。
    ///
    /// # 参数
    /// * `now` - 关闭时间
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 册进入已关闭。
    ///
    /// # 错误
    /// 非已发布时拒绝。已提交不得关闭为已关闭。
    pub fn close(&mut self, now: Instant, actor_id: &str) -> Result<()> {
        ensure_transition(self.status, BookletStatus::Closed)?;
        self.status = BookletStatus::Closed;
        self.closed_at = Some(now);
        self.link_revoked = true;
        self.touch(actor_id)
    }

    /// 撤销已提交选品册的链接访问。
    ///
    /// # 参数
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 不改变已提交状态，不删除方案。
    ///
    /// # 错误
    /// 非已提交时拒绝。
    pub fn revoke_access(&mut self, actor_id: &str) -> Result<()> {
        if self.status != BookletStatus::Submitted {
            return Err(Error::from("只有已提交的选品册可以撤销链接访问"));
        }
        self.link_revoked = true;
        self.touch(actor_id)
    }

    /// 发布前作废。
    ///
    /// # 参数
    /// * `now` - 作废时间
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 册进入已作废。
    ///
    /// # 错误
    /// 状态不允许作废时拒绝。
    pub fn void(&mut self, now: Instant, actor_id: &str) -> Result<()> {
        if !self.status.allows_void() {
            return Err(Error::from("当前状态不能作废"));
        }
        ensure_transition(self.status, BookletStatus::Voided)?;
        self.status = BookletStatus::Voided;
        self.voided_at = Some(now);
        self.touch(actor_id)
    }

    /// 提交成功：冻结册并关联唯一方案。
    ///
    /// # 参数
    /// * `proposal_id` - 方案身份
    /// * `now` - 提交时间
    ///
    /// # 返回
    /// 册进入已提交。
    ///
    /// # 错误
    /// 非已发布或已有方案时拒绝。
    pub fn mark_submitted(&mut self, proposal_id: SalesSelectionProposalId, now: Instant) -> Result<()> {
        if self.proposal_id.is_some() {
            return Err(Error::from("一本选品册只能有一份销售方案"));
        }
        ensure_transition(self.status, BookletStatus::Submitted)?;
        self.status = BookletStatus::Submitted;
        self.proposal_id = Some(proposal_id);
        self.submitted_at = Some(now);
        Ok(())
    }

    /// 判断服务端当前是否已到期。
    ///
    /// # 参数
    /// * `now` - 服务端时间
    ///
    /// # 返回
    /// 已到或过到期时间返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_expired(&self, now: Instant) -> bool {
        self.link_expires_at.is_some_and(|expires| now >= expires)
    }

    /// 判断持久化文档是否带齐组织范围字段。
    ///
    /// 清库漏集合时，组织范围落地前的选品册可能没有销售负责人或业务组织；
    /// 反序列化后为空，后台准备不得当作有效单据继续执行或写回。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 两个字段都非空时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn has_persisted_scope(&self) -> bool {
        !self.sales_owner_user_id.is_empty() && !self.business_org_unit_id.is_empty()
    }

    /// 公开写操作前校验令牌、状态与到期。
    ///
    /// # 参数
    /// * `token_hash` - 当前请求令牌哈希
    /// * `now` - 服务端时间
    ///
    /// # 返回
    /// 通过时返回 `Ok(())`。
    ///
    /// # 错误
    /// 令牌不匹配、已撤销、已到期或状态不允许写入时拒绝。
    pub fn ensure_public_write(&self, token_hash: &str, now: Instant) -> Result<()> {
        self.ensure_current_token(token_hash)?;
        if self.link_revoked || self.is_expired(now) {
            return Err(Error::from("选品已结束"));
        }
        if !self.status.allows_session_write() {
            return Err(Error::from("选品已结束"));
        }
        Ok(())
    }

    /// 校验请求令牌是否为当前有效哈希。
    ///
    /// # 参数
    /// * `token_hash` - 请求令牌哈希
    ///
    /// # 返回
    /// 匹配当前哈希时通过。
    ///
    /// # 错误
    /// 无链接或哈希不匹配时拒绝。
    pub fn ensure_current_token(&self, token_hash: &str) -> Result<()> {
        match self.link_token_hash.as_deref() {
            Some(current) if current == token_hash => Ok(()),
            _ => Err(Error::from("选品链接无效")),
        }
    }

    /// 乐观锁：请求版本必须等于当前版本。
    ///
    /// # 参数
    /// * `expected` - 请求携带的册版本
    ///
    /// # 返回
    /// 版本一致时通过。
    ///
    /// # 错误
    /// 基于旧版本时整次拒绝。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("选品册已被更新，请刷新后重试"));
        }
        Ok(())
    }

    /// 记录陈列删减，供仓储乐观锁递增版本。
    ///
    /// # 参数
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 操作人为空时拒绝。
    pub fn record_display_edit(&mut self, actor_id: &str) -> Result<()> {
        self.touch(actor_id)
    }

    /// 记录更新人。
    ///
    /// # 参数
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 操作人为空时拒绝。
    fn touch(&mut self, actor_id: &str) -> Result<()> {
        self.updated_by = normalize_actor(actor_id, "操作人不能为空")?;
        Ok(())
    }

    /// 清除活动任务槽。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    fn clear_task(&mut self) {
        self.active_task_id = None;
        self.pre_prepare_status = None;
    }

    /// 同册同时最多一个活动准备任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无活动任务时通过。
    ///
    /// # 错误
    /// 已有活动任务时拒绝。
    fn ensure_no_active_task(&self) -> Result<()> {
        if self.active_task_id.is_some() || self.status == BookletStatus::Preparing {
            return Err(Error::from("已有准备任务进行中，请等待完成后再试"));
        }
        Ok(())
    }
}

/// 按形态校验档位。
///
/// # 参数
/// * `form` - 选品形态
/// * `tiers` - 档位
///
/// # 返回
/// 单品返回空列表；套餐返回规范化档位。
///
/// # 错误
/// 单品带档位或套餐缺档位时拒绝。
fn normalize_form_tiers(form: SelectionForm, tiers: Vec<TierRule>) -> Result<Vec<TierRule>> {
    if form.is_package() {
        normalize_tiers(tiers)
    } else if tiers.is_empty() {
        Ok(Vec::new())
    } else {
        Err(Error::from("单品形态不填写档位"))
    }
}

/// 规范化操作人身份。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `empty_message` - 为空提示
///
/// # 返回
/// 返回去空白身份。
///
/// # 错误
/// 为空时拒绝。
fn normalize_actor(actor_id: &str, empty_message: &str) -> Result<String> {
    normalize_required_text(actor_id.to_string(), empty_message, 64, "操作人身份过长")
}

/// 为发布时间加上整天数。
///
/// # 参数
/// * `start` - 起点
/// * `days` - 天数
///
/// # 返回
/// 返回到期时刻。
///
/// # 错误
/// 无。
fn add_days(start: Instant, days: i64) -> Instant {
    Instant::from_unix_secs(start.unix_secs().saturating_add(days.saturating_mul(24 * 3600)))
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SkuId;

    use super::*;
    use crate::entity::sales_selection::{
        PoolFilterSnapshot, PoolSource, PoolSourceKind, PrepareKind, SelectionForm, SubmitMode,
    };

    /// 构造单品草稿。
    fn draft() -> SalesSelectionBooklet {
        SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("book-1"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("cust-1"),
                customer_no: "C1".into(),
                customer_name: "客户甲".into(),
                sales_owner_user_id: "sales-1".into(),
                business_org_unit_id: "org-1".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::ByQuantity,
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .expect("筛选来源合法"),
                tiers: Vec::new(),
                created_by: "u1".into(),
            },
        )
        .expect("草稿合法")
    }

    #[test]
    fn new_booklet_starts_as_draft() {
        let booklet = draft();
        assert_eq!(booklet.status, BookletStatus::Draft);
        assert_eq!(booklet.form, SelectionForm::SingleSku);
        assert_eq!(booklet.submit_mode, SubmitMode::ByQuantity);
    }

    #[test]
    fn reprepare_cannot_change_source_kind() {
        let mut booklet = draft();
        let selected = PoolSource::new(PoolSourceKind::Selection, None, Some(vec![SkuId::new("sku-1")]))
            .expect("勾选来源合法");
        assert!(booklet.apply_reprepare_rules(selected, Vec::new()).is_err());
    }

    #[test]
    fn stale_version_is_rejected() {
        let booklet = draft();
        assert!(booklet.ensure_version(booklet.base.version + 1).is_err());
        assert!(booklet.ensure_version(booklet.base.version).is_ok());
    }

    #[test]
    fn first_prepare_failure_restores_draft() {
        let mut booklet = draft();
        booklet.begin_prepare("task-1", PrepareKind::FirstPrepare, "u1").expect("草稿可首次准备");
        booklet.fail_prepare("筛选为空", "u1").expect("失败可恢复");
        assert_eq!(booklet.status, BookletStatus::Draft);
        assert_eq!(booklet.last_prepare_failure.as_deref(), Some("筛选为空"));
        assert!(booklet.active_task_id.is_none());
    }

    #[test]
    fn single_sku_cannot_regenerate_packages() {
        let mut booklet = draft();
        assert!(booklet.begin_prepare("task-1", PrepareKind::RegeneratedAll, "u1").is_err());
    }

    #[test]
    fn expired_link_rejects_public_write() {
        let mut booklet = draft();
        booklet.begin_prepare("task-1", PrepareKind::FirstPrepare, "u1").unwrap();
        booklet.complete_prepare("batch-1", BusinessDate::today(), Instant::now(), "u1").unwrap();
        booklet.publish("hash".into(), "cipher".into(), Instant::now(), "u1").unwrap();
        booklet.link_expires_at = Some(Instant::from_unix_secs(1));
        assert!(booklet.is_expired(Instant::now()));
        assert!(booklet.ensure_public_write("hash", Instant::now()).is_err());
    }

    #[test]
    fn legacy_document_defaults_missing_owner_and_org() {
        let booklet = draft();
        let mut value = serde_json::to_value(&booklet).expect("serialize booklet");
        let object = value.as_object_mut().expect("booklet object");
        object.remove("sales_owner_user_id");
        object.remove("business_org_unit_id");
        let restored: SalesSelectionBooklet =
            serde_json::from_value(value).expect("legacy booklet deserializes");
        assert!(!restored.has_persisted_scope());
        assert!(restored.sales_owner_user_id.is_empty());
        assert!(restored.business_org_unit_id.is_empty());
        assert!(draft().has_persisted_scope());
    }

    #[test]
    fn submitted_close_is_rejected() {
        let mut booklet = draft();
        booklet.begin_prepare("task-1", PrepareKind::FirstPrepare, "u1").unwrap();
        booklet.complete_prepare("batch-1", BusinessDate::today(), Instant::now(), "u1").unwrap();
        booklet.publish("hash".into(), "cipher".into(), Instant::now(), "u1").unwrap();
        booklet.mark_submitted(SalesSelectionProposalId::new("p1"), Instant::now()).unwrap();
        assert!(booklet.close(Instant::now(), "u1").is_err());
        assert!(booklet.revoke_access("u1").is_ok());
        assert_eq!(booklet.status, BookletStatus::Submitted);
    }
}
