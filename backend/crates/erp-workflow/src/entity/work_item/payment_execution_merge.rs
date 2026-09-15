//! 同一供应商付款执行任务的合并集合（W01 一键合并付款）。
//!
//! 任务仍按应付子账 1:1 存在；本值对象只约束一次打款允许覆盖的任务集合。
//! 不访问 I/O、不生成 ID、不读取时钟。

use std::collections::{HashMap, HashSet};

use erp_core::ids::WorkItemId;
use erp_core::{Error, Result};

/// 一次合并付款允许覆盖的最大任务数（含当前任务）。
pub const MAX_PAYMENT_EXECUTION_MERGE: usize = 50;

/// 已授权的一条付款执行任务成员事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentExecutionMergeMember {
    /// 开放付款执行任务。
    pub work_item_id: WorkItemId,
    /// 任务绑定的应付子账。
    pub payable_account_id: String,
    /// 应付所属供应商。
    pub supplier_id: String,
}

/// 一次打款覆盖的付款执行任务集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentExecutionMergeSet {
    members: Vec<PaymentExecutionMergeMember>,
}

impl PaymentExecutionMergeSet {
    /// 由已授权成员构造合并集合。
    ///
    /// # 参数
    /// * `members` - 当前打款要覆盖的付款执行任务；顺序由调用方冻结
    ///
    /// # 返回
    /// 成员合法时返回不可变集合。
    ///
    /// # 错误
    /// 空集、超过 50 条、任务或应付重复、或供应商不一致时失败关闭。
    ///
    /// # 关键业务约束
    /// 合并键是同一供应商，不是户名模糊匹配；跨供应商必须拒绝。
    pub fn try_new(members: Vec<PaymentExecutionMergeMember>) -> Result<Self> {
        if members.is_empty() {
            return Err(Error::from("付款执行任务不能为空"));
        }
        if members.len() > MAX_PAYMENT_EXECUTION_MERGE {
            return Err(Error::from("一次合并付款最多包含 50 条任务"));
        }
        let mut work_item_ids = HashSet::new();
        let mut payable_account_ids = HashSet::new();
        let supplier_id = members[0].supplier_id.as_str();
        for member in &members {
            if member.payable_account_id.trim().is_empty() {
                return Err(Error::from("合并付款任务缺少应付子账"));
            }
            if member.supplier_id.trim().is_empty() {
                return Err(Error::from("合并付款任务缺少供应商"));
            }
            if member.supplier_id != supplier_id {
                return Err(Error::from("合并付款只能包含同一供应商的付款任务"));
            }
            if !work_item_ids.insert(member.work_item_id.as_ref()) {
                return Err(Error::from("合并付款不能重复同一付款任务"));
            }
            if !payable_account_ids.insert(member.payable_account_id.as_str()) {
                return Err(Error::from("同一应付不能重复进入合并付款"));
            }
        }
        Ok(Self { members })
    }

    /// 返回冻结的成员切片。
    ///
    /// # 返回
    /// 构造时的成员顺序。
    ///
    /// # 错误
    /// 无。
    pub fn members(&self) -> &[PaymentExecutionMergeMember] {
        &self.members
    }

    /// 返回集合内唯一供应商。
    ///
    /// # 返回
    /// 首个成员的供应商 ID；构造时已保证全部相同。
    ///
    /// # 错误
    /// 无。
    pub fn supplier_id(&self) -> &str {
        self.members[0].supplier_id.as_str()
    }

    /// 返回是否覆盖多条付款任务。
    ///
    /// # 返回
    /// 成员多于 1 时为合并打款。
    ///
    /// # 错误
    /// 无。
    pub fn is_merged(&self) -> bool {
        self.members.len() > 1
    }

    /// 返回全部应付子账 ID，顺序与成员一致。
    ///
    /// # 返回
    /// 应付子账 ID 列表。
    ///
    /// # 错误
    /// 无。
    pub fn payable_account_ids(&self) -> Vec<&str> {
        self.members.iter().map(|member| member.payable_account_id.as_str()).collect()
    }

    /// 校验核销行覆盖且不超出本集合。
    ///
    /// # 参数
    /// * `allocation_account_ids` - 每条核销分录所属应付子账，顺序与提交行一致
    ///
    /// # 返回
    /// 每条已勾选任务至少有一行核销、且没有任何行落到集合外时成功。
    ///
    /// # 错误
    /// 核销为空、落到未勾选应付、或某条已勾选任务没有分配时失败关闭。
    ///
    /// # 关键业务约束
    /// 单任务保持原错误文案；合并打款使用明确的勾选范围文案。
    pub fn ensure_allocations_in_scope(&self, allocation_account_ids: &[String]) -> Result<()> {
        if allocation_account_ids.is_empty() {
            return Err(Error::from("至少提供一条核销分配"));
        }
        let allowed: HashSet<&str> =
            self.members.iter().map(|member| member.payable_account_id.as_str()).collect();
        for account_id in allocation_account_ids {
            if !allowed.contains(account_id.as_str()) {
                return Err(Error::from(self.out_of_scope_message()));
            }
        }
        let mut covered: HashMap<&str, bool> =
            self.members.iter().map(|member| (member.payable_account_id.as_str(), false)).collect();
        for account_id in allocation_account_ids {
            if let Some(flag) = covered.get_mut(account_id.as_str()) {
                *flag = true;
            }
        }
        if covered.values().any(|covered| !*covered) {
            return Err(Error::from(self.uncovered_member_message()));
        }
        Ok(())
    }

    /// 返回核销落到集合外时的稳定错误文案。
    ///
    /// # 返回
    /// 单任务与合并打款使用不同文案，便于既有测试与出纳操作对齐。
    ///
    /// # 错误
    /// 无。
    fn out_of_scope_message(&self) -> &'static str {
        if self.is_merged() {
            "一次付款只能核销已勾选付款任务对应应付中的分录"
        } else {
            "一次付款只能核销当前任务绑定应付子账中的分录"
        }
    }

    /// 返回已勾选任务缺少分配时的稳定错误文案。
    ///
    /// # 返回
    /// 单任务不会走到本分支的常规路径；合并打款要求每条任务都有金额。
    ///
    /// # 错误
    /// 无。
    fn uncovered_member_message(&self) -> &'static str {
        if self.is_merged() {
            "合并付款必须为每条已勾选任务分配付款金额"
        } else {
            "一次付款只能核销当前任务绑定应付子账中的分录"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(
        work_item_id: &str,
        payable_account_id: &str,
        supplier_id: &str,
    ) -> PaymentExecutionMergeMember {
        PaymentExecutionMergeMember {
            work_item_id: WorkItemId::new(work_item_id),
            payable_account_id: payable_account_id.to_string(),
            supplier_id: supplier_id.to_string(),
        }
    }

    /// 单任务集合是合法的退化形态，保持既有一任务一应付语义。
    #[test]
    fn single_member_set_is_allowed() {
        let set = PaymentExecutionMergeSet::try_new(vec![member("wi-1", "pa-1", "sup-1")]).unwrap();
        assert!(!set.is_merged());
        assert_eq!(set.supplier_id(), "sup-1");
        assert_eq!(set.payable_account_ids(), vec!["pa-1"]);
    }

    /// 同一供应商的多条任务可以进入一次打款。
    #[test]
    fn same_supplier_members_form_merged_set() {
        let set = PaymentExecutionMergeSet::try_new(vec![
            member("wi-1", "pa-1", "sup-1"),
            member("wi-2", "pa-2", "sup-1"),
        ])
        .unwrap();
        assert!(set.is_merged());
        assert_eq!(set.members().len(), 2);
    }

    /// 空集、超量、任务重复、应付重复和跨供应商都必须失败关闭。
    #[test]
    fn invalid_members_fail_closed() {
        assert!(PaymentExecutionMergeSet::try_new(Vec::new()).is_err());
        assert!(
            PaymentExecutionMergeSet::try_new(vec![
                member("wi-1", "pa-1", "sup-1"),
                member("wi-1", "pa-2", "sup-1"),
            ])
            .is_err()
        );
        assert!(
            PaymentExecutionMergeSet::try_new(vec![
                member("wi-1", "pa-1", "sup-1"),
                member("wi-2", "pa-1", "sup-1"),
            ])
            .is_err()
        );
        assert!(
            PaymentExecutionMergeSet::try_new(vec![
                member("wi-1", "pa-1", "sup-1"),
                member("wi-2", "pa-2", "sup-2"),
            ])
            .is_err()
        );
        let too_many = (0..MAX_PAYMENT_EXECUTION_MERGE + 1)
            .map(|index| member(&format!("wi-{index}"), &format!("pa-{index}"), "sup-1"))
            .collect();
        assert!(PaymentExecutionMergeSet::try_new(too_many).is_err());
    }

    /// 单任务核销落到其它应付时沿用原错误文案。
    #[test]
    fn single_task_rejects_foreign_allocation_with_legacy_message() {
        let set = PaymentExecutionMergeSet::try_new(vec![member("wi-1", "pa-1", "sup-1")]).unwrap();
        let err = set.ensure_allocations_in_scope(&["pa-2".to_string()]).unwrap_err();
        assert!(err.to_string().contains("一次付款只能核销当前任务绑定应付子账中的分录"));
    }

    /// 合并打款要求每条已勾选任务都有核销，且禁止集合外分录。
    #[test]
    fn merged_set_requires_each_member_and_rejects_outsiders() {
        let set = PaymentExecutionMergeSet::try_new(vec![
            member("wi-1", "pa-1", "sup-1"),
            member("wi-2", "pa-2", "sup-1"),
        ])
        .unwrap();
        set.ensure_allocations_in_scope(&["pa-1".to_string(), "pa-2".to_string()]).unwrap();
        let missing = set.ensure_allocations_in_scope(&["pa-1".to_string()]).unwrap_err();
        assert!(missing.to_string().contains("合并付款必须为每条已勾选任务分配付款金额"));
        let outsider =
            set.ensure_allocations_in_scope(&["pa-1".to_string(), "pa-3".to_string()]).unwrap_err();
        assert!(outsider.to_string().contains("一次付款只能核销已勾选付款任务对应应付中的分录"));
        assert!(set.ensure_allocations_in_scope(&[]).is_err());
    }
}
