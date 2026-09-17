//! 结算单状态机：状态枚举、守卫与流转。

use super::*;

/// 结算单状态（数据模型 §6.20：草稿、待对账、有差异、待复核、已确认、已作废）。
///
/// 固定枚举（§4.6），不属于数据模型第 7 章的固定状态机；结算确认编排（§8.4 第 6 条）
/// 由 P3 承担。实体层固化保守守卫：已作废为终态，已确认只能作废。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementStatus {
    /// 草稿。
    Draft,
    /// 待对账。
    PendingReconciliation,
    /// 有差异。
    HasDifference,
    /// 待复核。
    PendingReview,
    /// 已确认：同事务形成应付（§8.4 第 6 条，P3）。
    Confirmed,
    /// 已作废：终态。
    Voided,
}

impl SettlementStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingReconciliation => "待对账",
            Self::HasDifference => "有差异",
            Self::PendingReview => "待复核",
            Self::Confirmed => "已确认",
            Self::Voided => "已作废",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingReconciliation => "PENDING_RECONCILIATION",
            Self::HasDifference => "HAS_DIFFERENCE",
            Self::PendingReview => "PENDING_REVIEW",
            Self::Confirmed => "CONFIRMED",
            Self::Voided => "VOIDED",
        }
    }
}

impl SupplierSettlementStatement {
    /// 判断结算单是否仍处于可编辑草稿阶段。
    ///
    /// # 返回
    /// 草稿、待对账或有差异状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        matches!(
            self.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        )
    }

    /// 判断结算单是否已经作废。
    ///
    /// # 返回
    /// 状态为 `VOIDED` 时返回 `true`。
    pub fn is_voided(&self) -> bool {
        self.status == SettlementStatus::Voided
    }

    /// 判断结算单是否正在等待财务复核。
    ///
    /// # 返回
    /// 状态为 `PENDING_REVIEW` 时返回 `true`。
    pub fn is_pending_review(&self) -> bool {
        self.status == SettlementStatus::PendingReview
    }
    /// 作废尚未提交复核的可编辑结算草稿。
    ///
    /// # 返回
    /// 状态推进到 `VOIDED` 时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前结算单已提交复核或进入终态时返回领域错误。
    pub fn void_draft(&mut self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("当前结算状态禁止作废草稿"));
        }
        self.status = SettlementStatus::Voided;
        Ok(())
    }
    /// 将当前冻结主题提交财务复核。
    ///
    /// 本方法只形成结算单状态事实；应用服务必须把唯一复核任务与审计写入同一事务。
    ///
    /// # 错误
    /// 非草稿、待对账或有差异状态时返回错误。
    pub fn submit_review(&mut self) -> Result<()> {
        if !matches!(
            self.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        ) {
            return Err(Error::from("当前结算状态不允许提交复核"));
        }
        self.status = SettlementStatus::PendingReview;
        Ok(())
    }
    /// 应用状态更新并维护确认字段。
    ///
    /// # 参数
    /// * `status` - 新的状态
    /// * `payable_account_id` - 应付账户（推进到已确认时必填）
    ///
    /// # 错误
    /// 推进到已确认但缺少应付账户时返回错误。
    pub(super) fn apply_status(
        &mut self,
        status: SettlementStatus,
        payable_account_id: Option<PayableAccountId>,
    ) -> Result<()> {
        if status == SettlementStatus::Confirmed {
            let payable_account_id =
                payable_account_id.ok_or_else(|| Error::from("确认时必须提供应付账户"))?;
            self.confirmed_at.get_or_insert_with(Instant::now);
            self.payable_account_id = Some(payable_account_id);
        }
        self.status = status;
        Ok(())
    }
}

/// 校验结算状态迁移守卫（保守守卫：已作废终态，已确认只能作废；幂等恒合法）。
///
/// # 参数
/// * `from` - 迁移前状态
/// * `to` - 目标状态
///
/// # 错误
/// 已作废再变更、或已确认迁移到已确认以外的状态时返回错误。
pub(super) fn ensure_status_move(from: SettlementStatus, to: SettlementStatus) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if from == SettlementStatus::Voided {
        return Err(Error::from("已作废结算单不可再变更状态"));
    }
    if from == SettlementStatus::Confirmed && to != SettlementStatus::Voided {
        return Err(Error::from("已确认结算单只能作废"));
    }
    Ok(())
}
