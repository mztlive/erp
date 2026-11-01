//! 结算单对账负责人交接与差异处理人改派。

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};

use super::statement::{ACTOR_MAX_LEN, SupplierSettlementStatement};

impl SupplierSettlementStatement {
    /// 显式交接对账负责人及可选业务组织；不改派开放复核任务。
    ///
    /// # 参数
    /// * `target_user_id` - 目标对账负责人
    /// * `target_org_unit_id` - 显式目标组织；省略则保留原组织
    ///
    /// # 返回
    /// 交接成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标为空、与当前复核人相同、组织为空或与当前责任完全一致时拒绝。
    ///
    /// # 关键业务约束
    /// 经办≠复核保持；差异处理人独立，不随对账负责人隐式改派；组织不随接收人部门变化。
    pub fn handover(&mut self, target_user_id: String, target_org_unit_id: Option<String>) -> Result<()> {
        let target = normalize_required_text(
            target_user_id,
            "目标对账负责人不能为空",
            ACTOR_MAX_LEN,
            "目标对账负责人过长",
        )?;
        let next_org = match target_org_unit_id {
            Some(org) => {
                normalize_required_text(org, "目标业务组织不能为空", ACTOR_MAX_LEN, "目标业务组织过长")?
            },
            None => self.business_org_unit_id.clone(),
        };
        if next_org.is_empty() {
            return Err(Error::from("对账负责人缺少有效业务组织"));
        }
        if self.reviewed_by.as_deref() == Some(target.as_str()) {
            return Err(Error::from("经办人与复核人不得相同"));
        }
        if target == self.prepared_by && next_org == self.business_org_unit_id {
            return Err(Error::from("目标已是当前对账负责人，无需交接"));
        }
        self.prepared_by = target;
        self.business_org_unit_id = next_org;
        Ok(())
    }

    /// 独立改派差异处理人，不改对账负责人或复核人。
    ///
    /// # 参数
    /// * `target_user_id` - 目标差异处理人
    ///
    /// # 返回
    /// 改派成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标为空、超长或与当前差异处理人相同时拒绝。
    ///
    /// # 关键业务约束
    /// 不得把 `resolved_by` 强制写成对账负责人；本命令只改单据差异处理人。
    pub fn reassign_difference_handler(&mut self, target_user_id: String) -> Result<()> {
        let target = normalize_required_text(
            target_user_id,
            "目标差异处理人不能为空",
            ACTOR_MAX_LEN,
            "目标差异处理人过长",
        )?;
        if target == self.difference_handler() {
            return Err(Error::from("目标已是当前差异处理人，无需改派"));
        }
        self.difference_handler_user_id = target;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{SupplierAccountId, SupplierSettlementStatementId};
    use erp_core::money::Amount;

    use super::super::statement::{SupplierSettlementStatement, SupplierSettlementStatementData};

    fn sample() -> SupplierSettlementStatement {
        SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-handover"),
            SupplierSettlementStatementData {
                statement_no: "ST-H-1".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".into(),
                period_policy_version: "1".into(),
                period_timezone: "Asia/Shanghai".into(),
                external_bill_no: None,
                external_bill_version: None,
                erp_amount: Amount::from_str("1.00").unwrap(),
                supplier_amount: Amount::from_str("1.00").unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1),
                source_snapshot_at: Instant::from_unix_secs(1),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: "cutoff".into(),
                refresh_cutoff_policy_version: "1".into(),
                prepared_by: "owner-a".into(),
                business_org_unit_id: "org-a".into(),
                difference_handler_user_id: String::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn handover_keeps_difference_handler_and_rejects_reviewer() {
        let mut statement = sample();
        statement.reviewed_by = Some("reviewer-b".into());
        assert!(statement.handover("reviewer-b".into(), None).is_err());
        statement.handover("owner-b".into(), Some("org-b".into())).unwrap();
        assert_eq!(statement.prepared_by, "owner-b");
        assert_eq!(statement.business_org_unit_id, "org-b");
        assert_eq!(statement.difference_handler(), "owner-a");
    }

    #[test]
    fn difference_handler_reassign_is_independent() {
        let mut statement = sample();
        statement.reassign_difference_handler("handler-c".into()).unwrap();
        assert_eq!(statement.difference_handler(), "handler-c");
        assert_eq!(statement.prepared_by, "owner-a");
        assert!(statement.is_difference_handler("handler-c"));
        assert!(!statement.is_prepared_by("handler-c"));
    }
}
