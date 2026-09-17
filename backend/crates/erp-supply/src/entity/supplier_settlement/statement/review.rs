//! 结算复核决定：复核结果、决定与确认校验。

use super::*;

/// 结算复核的正式决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewResult {
    /// 复核确认并形成应付。
    Confirmed,
    /// 驳回给经办人继续处理。
    Rejected,
}
/// 结算复核正式决定数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettlementReviewDecision {
    /// 确认结算。
    Confirm {
        /// 同事务形成的应付账户。
        payable_account_id: PayableAccountId,
        /// 复核说明。
        comment: Option<String>,
    },
    /// 驳回复核。
    Reject {
        /// 驳回后的可编辑状态，只允许草稿或有差异。
        return_status: SettlementStatus,
        /// 结构化驳回原因（固定三元值对象，绕过 Service 也不能写入未知原因）。
        reason_code: SettlementReviewRejectReason,
        /// 补充说明。
        comment: Option<String>,
    },
}

impl SupplierSettlementStatement {
    /// 校验客户端提交的复核主题与刷新截止策略快照。
    ///
    /// # 参数
    /// * `subject_hash` - 客户端持有的主题摘要
    /// * `cutoff_policy_id` - 客户端持有的刷新截止策略
    /// * `cutoff_policy_version` - 客户端持有的策略版本
    ///
    /// # 返回
    /// 三项均与当前冻结值一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一值不一致时返回领域错误。
    pub fn ensure_review_snapshot(
        &self,
        subject_hash: &str,
        cutoff_policy_id: &str,
        cutoff_policy_version: &str,
    ) -> Result<()> {
        if subject_hash != self.subject_hash
            || cutoff_policy_id != self.refresh_cutoff_policy_id
            || cutoff_policy_version != self.refresh_cutoff_policy_version
        {
            return Err(Error::from("结算主题或刷新截止策略不一致"));
        }
        Ok(())
    }
    /// 校验当前主题摘要与全部差异正式结论一致且没有待处理差异。
    ///
    /// # 参数
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 差异均已处理且主题摘要一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 存在待处理差异或主题摘要过期时返回领域错误。
    pub fn ensure_resolved_subject(&self, differences: &[SupplierSettlementDifference]) -> Result<()> {
        if differences.iter().any(|difference| difference.status == SettlementDifferenceStatus::Pending) {
            return Err(Error::from("存在未解决差异，禁止提交或确认结算"));
        }
        if self.subject_hash != self.review_subject_hash(differences) {
            return Err(Error::from("结算主题摘要与当前差异结论不一致"));
        }
        Ok(())
    }
    /// 计算 ERP 接受差异对应的冻结成本差额。
    ///
    /// 每个结算明细的 ERP 接受含税差异合计必须精确等于供应商账单与 ERP 冻结
    /// 含税差额；不含税与税额只从冻结三元组派生，禁止按含税金额猜测。
    ///
    /// # 参数
    /// * `items` - 当前结算单的冻结明细
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 返回汇总后的含税、不含税和税额差额。
    ///
    /// # 错误
    /// 明细重复、差异指向其他明细或金额恒等不成立时返回领域错误。
    pub fn accepted_cost_delta(
        &self,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
    ) -> Result<SettlementCostDelta> {
        let statement_id = erp_core::ids::SupplierSettlementStatementId::new(self.base.id.as_str());
        let mut item_by_id = HashMap::with_capacity(items.len());
        for item in items {
            if !item.belongs_to_statement(&statement_id) {
                return Err(Error::from("结算快照包含其他结算单明细"));
            }
            if item_by_id.insert(item.base.id.as_str(), item).is_some() {
                return Err(Error::from("结算快照包含重复明细"));
            }
        }

        let mut accepted_gross_by_item: HashMap<&str, Amount> = HashMap::new();
        for difference in differences
            .iter()
            .filter(|difference| difference.status == SettlementDifferenceStatus::ErpAcknowledged)
        {
            let item_id = difference.statement_item_id.as_ref();
            if !item_by_id.contains_key(item_id) {
                return Err(Error::from("ERP接受差异未指向当前结算快照明细"));
            }
            accepted_gross_by_item
                .entry(item_id)
                .and_modify(|amount| *amount = amount.checked_add(difference.difference_amount))
                .or_insert(difference.difference_amount);
        }

        let mut total = SettlementCostDelta::zero();
        for (item_id, accepted_gross) in accepted_gross_by_item {
            let delta = item_by_id[item_id].supplier_minus_erp_delta()?;
            if accepted_gross != delta.gross {
                return Err(Error::from(format!("结算明细 {item_id} 的 ERP 接受差异与冻结双方金额不一致")));
            }
            total.add_assign(delta);
        }
        total.validate()?;
        Ok(total)
    }
    /// 校验结算单具备正式确认条件并返回成本差额。
    ///
    /// # 参数
    /// * `items` - 当前结算单的冻结明细
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 账单身份完整、差异已解决且主题一致时返回冻结成本差额。
    ///
    /// # 错误
    /// 外部账单身份不完整、差异未解决、主题过期或成本差额不一致时返回错误。
    pub fn ensure_confirmable(
        &self,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
    ) -> Result<SettlementCostDelta> {
        if items.is_empty() {
            return Err(Error::from("结算单没有冻结明细"));
        }
        if self.external_bill_no.is_none() || self.external_bill_version.is_none() {
            return Err(Error::from("供应商账单身份未完整冻结"));
        }
        self.ensure_resolved_subject(differences)?;
        self.accepted_cost_delta(items, differences)
    }
    /// 标记当前可编辑结算草稿仍存在正式差异。
    ///
    /// # 返回
    /// 状态推进到 `HAS_DIFFERENCE` 时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前结算单已提交复核或进入终态时返回领域错误。
    pub fn mark_has_difference(&mut self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("当前结算状态禁止登记差异结论"));
        }
        self.status = SettlementStatus::HasDifference;
        Ok(())
    }
    /// 记录强类型结算复核决定。
    ///
    /// 确认形成应付并进入终态；驳回必须携带结构化原因，且只能回到草稿或有差异。
    /// 经办人与复核人岗位分离由本实体再次固化。
    ///
    /// # 错误
    /// 非待复核状态、岗位冲突、驳回原因非法或目标状态非法时返回错误。
    pub fn record_review(
        &mut self,
        decision: SettlementReviewDecision,
        reviewed_by: impl Into<String>,
        reviewed_at: Instant,
    ) -> Result<()> {
        if self.status != SettlementStatus::PendingReview {
            return Err(Error::from("仅待复核结算单可以记录正式决定"));
        }
        let reviewed_by =
            normalize_required_text(reviewed_by.into(), "复核人不能为空", ACTOR_MAX_LEN, "复核人过长")?;
        if reviewed_by == self.prepared_by {
            return Err(Error::from("经办人与复核人不得相同"));
        }
        let (result, target_status, reason_code, comment, payable_account_id) = match decision {
            SettlementReviewDecision::Confirm { payable_account_id, comment } => (
                SettlementReviewResult::Confirmed,
                SettlementStatus::Confirmed,
                None,
                normalize_optional_text(comment, "复核说明", REVIEW_COMMENT_MAX_LEN)?,
                Some(payable_account_id),
            ),
            SettlementReviewDecision::Reject { return_status, reason_code, comment } => {
                if !matches!(return_status, SettlementStatus::Draft | SettlementStatus::HasDifference) {
                    return Err(Error::from("驳回复核只能退回草稿或有差异状态"));
                }
                (
                    SettlementReviewResult::Rejected,
                    return_status,
                    Some(reason_code.as_str().to_string()),
                    normalize_optional_text(comment, "复核说明", REVIEW_COMMENT_MAX_LEN)?,
                    None,
                )
            },
        };
        self.status = target_status;
        self.reviewed_by = Some(reviewed_by);
        self.review_result = Some(result);
        self.review_reason_code = reason_code;
        self.review_comment = comment;
        self.reviewed_at = Some(reviewed_at);
        self.payable_account_id = payable_account_id;
        self.confirmed_at = (result == SettlementReviewResult::Confirmed).then_some(reviewed_at);
        Ok(())
    }
    /// 应用复核人更新。
    ///
    /// # 参数
    /// * `reviewed_by` - 新的复核人
    ///
    /// # 错误
    /// 复核人为空/超长或与经办人相同时返回错误。
    pub(super) fn apply_reviewed_by(&mut self, reviewed_by: String) -> Result<()> {
        let reviewed_by =
            normalize_required_text(reviewed_by, "复核人不能为空", ACTOR_MAX_LEN, "复核人过长")?;
        if reviewed_by == self.prepared_by {
            return Err(Error::from("经办人与复核人不得相同"));
        }
        self.reviewed_by = Some(reviewed_by);
        Ok(())
    }
}
