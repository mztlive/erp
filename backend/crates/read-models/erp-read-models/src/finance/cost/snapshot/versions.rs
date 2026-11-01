//! 完整范围的轻量版本读取；身份、来源和顺序与首次金额快照一致。
use erp_finance::repository::cost::read_scope::{CostAllocationScopeVersion, CostEntryScopeVersion};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;

use super::*;

pub(super) struct VersionFacts {
    pub entries: Vec<CostEntryScopeVersion>,
    allocations: Vec<CostAllocationScopeVersion>,
}

impl VersionFacts {
    /// 首次成本快照直接复用已读事实，不追加一轮版本数据库读取。
    ///
    /// # 参数
    /// 完整候选成本和分配，保持仓储返回次序。
    /// # 返回
    /// 返回不含金额和展示字段的范围版本事实。
    /// # 错误
    /// 无；转换不执行 I/O。
    pub fn from_rows(entries: &[CostEntryRow], allocations: &[CostAllocation]) -> Self {
        Self {
            entries: entries
                .iter()
                .map(|row| CostEntryScopeVersion { id: row.id.clone(), version: row.version })
                .collect(),
            allocations: allocations
                .iter()
                .map(|row| CostAllocationScopeVersion {
                    id: row.base.id.clone(),
                    version: row.base.version,
                    cost_entry_id: row.cost_entry_id.to_string(),
                    sales_order_id: row.sales_order_id.as_ref().map(ToString::to_string),
                })
                .collect(),
        }
    }

    /// 原指纹先关联销售，再完整分配，最后完整成本；不能改成当前页版本。
    ///
    /// # 参数
    /// * `fingerprint` - 已绑定授权和关联销售版本的哈希器。
    /// # 返回
    /// 将全部分配及成本的身份和版本追加到原哈希器。
    /// # 错误
    /// 无。
    pub fn hash(&self, fingerprint: &mut DefaultHasher) {
        for line in &self.allocations {
            line.id.hash(fingerprint);
            line.version.hash(fingerprint);
        }
        for row in &self.entries {
            row.id.hash(fingerprint);
            row.version.hash(fingerprint);
        }
    }
}

impl CostReadModel {
    /// 返回前新事务只读取身份、版本和授权来源，不计算第二份金额与页面。
    ///
    /// # 参数
    /// 原业务筛选、读取选择、资源动作及操作人。
    /// # 返回
    /// 返回当前完整范围的指纹。
    /// # 错误
    /// 无权限、非法来源筛选、候选超限或持久化失败时拒绝。
    pub(super) async fn current_version(
        &self,
        params: &CostEntryListParams,
        selection: &CostSelection,
        resource: &str,
        action: &str,
        actor: &AuditActor,
    ) -> Result<String> {
        let filter = cost_entry_filter(params)?;
        let this = self.clone();
        let actor = actor.clone();
        let id = selection.id.clone();
        let resource = resource.to_owned();
        let action = action.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut access = this.authorize(&actor, &resource, &action, executor).await?;
                    access.require_filter(&filter)?;
                    if !access.empty() {
                        let facts = this.candidate_versions(&filter, id.as_deref(), executor).await?;
                        this.authorized_orders(&facts, &mut access, executor).await?;
                        facts.hash(&mut access.fingerprint);
                    }
                    Ok(format!("{:x}", access.fingerprint.finish()))
                })
            })
            .await
    }

    /// 保留完整业务筛选集合，轻投影的上限与原金额快照相同。
    ///
    /// # 参数
    /// 原成本业务筛选、可选成本身份和调用方事务执行器。
    /// # 返回
    /// 返回完整成本身份版本及分配来源版本，保持原批次次序。
    /// # 错误
    /// 候选超过原成本或分配上限、持久化失败时拒绝。
    pub(super) async fn candidate_versions(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<VersionFacts> {
        let entries = self.db.cost_entries().scope_candidate_versions(filter, id, executor).await?;
        if entries.len() > ENTRY_LIMIT {
            return Err(Error::ValidationError("成本查询超过上限，请指定供应商或成本类型".into()));
        }
        let ids = entries.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let mut allocations = Vec::new();
        for chunk in ids.chunks(500) {
            allocations.extend(self.db.cost_allocations().scope_allocation_versions(chunk, executor).await?);
            if allocations.len() > ALLOCATION_LIMIT {
                return Err(Error::ValidationError("成本分配超过查询上限，请收窄业务条件".into()));
            }
        }
        Ok(VersionFacts { entries, allocations })
    }

    /// 相同销售范围读取窄身份版本；包含所有原候选分配的获授权来源。
    ///
    /// # 参数
    /// 完整候选版本、当前授权与调用方事务执行器。
    /// # 返回
    /// 返回获授权来源身份，并按原身份顺序将其版本写入授权指纹。
    /// # 错误
    /// 持久化读取或反序列化失败时返回错误。
    pub(super) async fn authorized_orders(
        &self,
        facts: &VersionFacts,
        access: &mut CostAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        let ids = facts
            .allocations
            .iter()
            .filter_map(|line| line.sales_order_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut orders = BTreeSet::new();
        for chunk in ids.chunks(500) {
            let current = self
                .db
                .sales_orders()
                .scope_order_versions(chunk, &access.sales, &access.sales, executor)
                .await?;
            for order in current {
                order.id.hash(&mut access.fingerprint);
                order.version.hash(&mut access.fingerprint);
                orders.insert(order.id);
            }
        }
        Ok(orders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> VersionFacts {
        VersionFacts {
            entries: vec![
                CostEntryScopeVersion { id: "cost-a".into(), version: 1 },
                CostEntryScopeVersion { id: "cost-b".into(), version: 3 },
            ],
            allocations: vec![
                CostAllocationScopeVersion {
                    id: "allocation-a".into(),
                    version: 2,
                    cost_entry_id: "cost-a".into(),
                    sales_order_id: Some("sales-a".into()),
                },
                CostAllocationScopeVersion {
                    id: "allocation-b".into(),
                    version: 4,
                    cost_entry_id: "cost-b".into(),
                    sales_order_id: Some("sales-b".into()),
                },
            ],
        }
    }

    fn version(facts: &VersionFacts) -> u64 {
        let mut fingerprint = DefaultHasher::new();
        facts.hash(&mut fingerprint);
        fingerprint.finish()
    }

    #[test]
    fn complete_scope_fingerprint_detects_unreturned_cost_and_allocation_changes() {
        let original = version(&facts());
        let mut changed = facts();
        changed.entries[1].version += 1;
        assert_ne!(original, version(&changed));
        let mut changed = facts();
        changed.allocations[1].version += 1;
        assert_ne!(original, version(&changed));
        let mut changed = facts();
        changed.entries.pop();
        assert_ne!(original, version(&changed));
        let mut changed = facts();
        changed.allocations.pop();
        assert_ne!(original, version(&changed));
        let mut changed = facts();
        changed.entries.push(CostEntryScopeVersion { id: "new-cost".into(), version: 1 });
        assert_ne!(original, version(&changed));
    }

    #[test]
    fn initial_amount_facts_and_minimal_versions_produce_identical_scope_fingerprint() {
        use super::super::tests::{allocation, entry};
        let mut entries = vec![entry("cost-a"), entry("unreturned-cost")];
        entries[1].version = 3;
        let mut allocations = vec![allocation("a", "sales-a", "60"), allocation("b", "sales-b", "40")];
        allocations[0].base.version = 2;
        allocations[1].base.version = 4;
        let original = VersionFacts::from_rows(&entries, &allocations);
        let minimal = VersionFacts {
            entries: serde_json::from_value(serde_json::json!([
                { "id": "cost-a", "version": 1 }, { "id": "unreturned-cost", "version": 3 }
            ]))
            .unwrap(),
            allocations: serde_json::from_value(serde_json::json!([
                { "id": "a", "version": 2, "cost_entry_id": "cost-a", "sales_order_id": "sales-a" },
                { "id": "b", "version": 4, "cost_entry_id": "cost-a", "sales_order_id": "sales-b" }
            ]))
            .unwrap(),
        };
        assert_eq!(version(&original), version(&minimal));
        let mut changed = allocations;
        changed[1].base.version += 1;
        assert_ne!(version(&original), version(&VersionFacts::from_rows(&entries, &changed)));
    }
}
