//! 成本范围投影所需的有界候选事实；应用层在分页前裁剪全部分配。
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::{QueryModel, QueryOutput};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Result, aggregate, mongo_ops};
use serde::Deserialize;

use super::{CostEntryFilter, CostEntryRow, cost_entry_projection};
use crate::entity::cost::{CostAllocation, CostEntry};

/// 候选成本上限，超限由调用方整体拒绝，禁止截断合计。
pub const ENTRY_LIMIT: usize = 10_000;
/// 候选分配上限。
pub const ALLOCATION_LIMIT: usize = 100_000;

/// 成本范围版本事实；重验不读取金额及完整来源引用。
#[derive(Debug, Clone, Deserialize)]
pub struct CostEntryScopeVersion {
    /// 成本身份。
    pub id: String,
    /// 成本业务版本。
    pub version: u64,
}

/// 分配版本及授权所需的真实销售来源，不包含金额或展示字段。
#[derive(Debug, Clone, Deserialize)]
pub struct CostAllocationScopeVersion {
    /// 分配身份。
    pub id: String,
    /// 分配业务版本。
    pub version: u64,
    /// 分配对应的成本身份。
    pub cost_entry_id: String,
    /// 真实销售来源，缺失来源保留 None。
    pub sales_order_id: Option<String>,
}

#[allow(async_fn_in_trait)]
pub trait CostEntryReadScopeExt {
    /// 装载业务筛选下完整的有界候选成本，不在权限裁剪前分页。
    ///
    /// # 参数
    /// * `filter` - 已规范化的成本筛选。
    /// * `id` - 精确成本身份；`None` 表示不按身份收窄。
    /// * `access` - 本入口固定根授权，数据库在候选上限前执行完整会员过滤。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 最多上限加一条；精确 ID 只用于独立详情。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_candidates(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<CostEntryRow>>;

    /// 读取完整候选身份和版本；保留新增、删除及非当前页变化检测。
    ///
    /// # 参数
    /// * `filter` - 已规范化的成本筛选。
    /// * `id` - 独立成本筛选。
    /// * `access` - 与候选查询相同的固定根授权。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 最多候选上限加一条，按身份稳定排序。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_candidate_versions(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<CostEntryScopeVersion>>;

    /// 只装载匹配分配引用的成本投影，空身份集合直接返回空集。
    ///
    /// # 参数
    /// * `filter` - 已规范化的成本筛选。
    /// * `ids` - 当前完整候选集合内的成本身份。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 返回匹配成本，按身份稳定排序。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_entries_by_ids(
        &self,
        filter: &CostEntryFilter,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryRow>>;

    /// 在完整真实分配来源投影上执行统一授权阶段，只返回完整金额会员身份。
    /// # 参数
    /// ids 为服务端完整候选身份，stages 为 identity 完整视图编译结果，executor 为原事务。
    /// # 返回
    /// 当前获准读取整笔成本的身份和版本；空候选直接返回空集。
    /// # 错误
    /// 聚合或投影解码失败时保留仓储错误。
    async fn scope_whole_ids(
        &self,
        ids: &[String],
        stages: &[Document],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryScopeVersion>>;
}

impl CostEntryReadScopeExt for persistence_core::Repository<'_, CostEntry> {
    async fn scope_whole_ids(
        &self,
        ids: &[String],
        stages: &[Document],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryScopeVersion>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut pipeline = vec![doc! { "$match": { "deleted_at": 0, "id": { "$in": ids } } }];
        pipeline.extend(whole_source_stages());
        pipeline.extend_from_slice(stages);
        pipeline.push(doc! { "$match": { "scope_whole": true } });
        pipeline.push(doc! { "$project": { "_id": 0, "id": 1, "version": 1 } });
        aggregate::rows(&self.collection().clone_with_type::<CostEntryScopeVersion>(), pipeline, executor)
            .await
    }

    async fn scope_candidates(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<CostEntryRow>> {
        let prefix = authorized_membership(access, self.collection().name(), executor)?;
        let mut document = filter.to_doc();
        if let Some(id) = id {
            document.insert("id", id);
        }
        let options = FindOptions::builder()
            .limit(10_001)
            .sort(doc! { "id": 1 })
            .projection(cost_entry_projection())
            .build();
        let rows = aggregate::find(
            &self.collection().clone_with_type::<CostEntryRow>(),
            document,
            options,
            prefix,
            executor,
        )
        .await?;
        access.ensure_current(executor)?;
        Ok(rows)
    }

    async fn scope_candidate_versions(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<CostEntryScopeVersion>> {
        let prefix = authorized_membership(access, self.collection().name(), executor)?;
        let mut document = filter.to_doc();
        if let Some(id) = id {
            document.insert("id", id);
        }
        let options = FindOptions::builder()
            .limit(10_001)
            .sort(doc! { "id": 1 })
            .projection(doc! { "id": 1, "version": 1 })
            .build();
        let rows = aggregate::find(
            &self.collection().clone_with_type::<CostEntryScopeVersion>(),
            document,
            options,
            prefix,
            executor,
        )
        .await?;
        access.ensure_current(executor)?;
        Ok(rows)
    }

    async fn scope_entries_by_ids(
        &self,
        filter: &CostEntryFilter,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryRow>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let mut document = filter.to_doc();
        document.insert("id", doc! { "$in": ids });
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostEntryRow>(),
            document,
            FindOptions::builder()
                .limit((ENTRY_LIMIT + 1) as i64)
                .sort(doc! { "id": 1 })
                .projection(cost_entry_projection())
                .build(),
            executor,
        )
        .await
    }
}

/// 用同一实际成本身份做授权会员关联，保留外根的内部计算投影。
fn authorized_membership(
    access: &CompiledAccess,
    collection: &str,
    executor: &mut dyn Executor,
) -> crate::Result<Vec<Document>> {
    ensure_candidate_model(access.model(), collection)?;
    access.ensure_current(executor)?;
    let whole = if !matches!(access.model().output, QueryOutput::Object) {
        access.whole_stages()?
    } else {
        access.stages()
    };
    Ok(membership_stages(collection, access.stages(), whole))
}

/// 候选只使用固定成本身份会员能力；存在性不能附带完整金额字段。
fn ensure_candidate_model(model: &QueryModel, collection: &str) -> crate::Result<()> {
    model.validate()?;
    if model.collection != collection
        || !matches!(model.resource.as_str(), "cost_entry" | "cost_allocation")
        || model.id_field != "id"
    {
        return Err(crate::Error::ValidationError("成本候选授权模型不匹配".into()));
    }
    Ok(())
}

fn membership_stages(collection: &str, shares: &[Document], whole: &[Document]) -> Vec<Document> {
    let branch = |stages: &[Document]| {
        let mut branch = stages.to_vec();
        branch.extend([doc! { "$project": { "_id": 0, "id": 1 } }, doc! { "$limit": 1 }]);
        branch
    };
    let pipeline = vec![
        doc! { "$match": { "$expr": { "$eq": ["$id", "$$root"] } } },
        doc! { "$facet": { "shares": branch(shares), "whole": branch(whole) } },
        doc! { "$match": { "$expr": { "$or": [
            { "$gt": [{ "$size": "$shares" }, 0] }, { "$gt": [{ "$size": "$whole" }, 0] }
        ] } } },
        doc! { "$project": { "_id": 0, "id": "$$root" } },
    ];
    vec![
        doc! { "$lookup": { "from": collection, "let": { "root": "$id" }, "pipeline": pipeline, "as": "__cost_authorized_root" } },
        doc! { "$match": { "__cost_authorized_root.0": { "$exists": true } } },
        doc! { "$unset": "__cost_authorized_root" },
    ]
}

#[allow(async_fn_in_trait)]
pub trait CostAllocationReadScopeExt {
    /// 批量读取成本对应分配，调用方必须检查上限并跨批次累计。
    ///
    /// # 参数
    /// * `ids` - 成本身份集合。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回最多分配上限加一条；空成本集合返回空集。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>>;

    /// 读取成本对应的完整分配版本及销售来源，调用方累计检查上限。
    ///
    /// # 参数
    /// * `ids` - 成本身份集合。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 最多分配上限加一条；空成本集合返回空集。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_allocation_versions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>>;

    /// 联合应用销售单与成本身份筛选，再读取匹配分配的成本引用。
    ///
    /// # 参数
    /// * `cost_entry_id` - 可选成本身份。
    /// * `sales_order_id` - 可选销售单身份。
    /// * `entry_ids` - 同事务中的完整候选成本身份，排除孤立分配。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 最多分配上限加一条，未分页的完整匹配集合。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_matching_allocations(
        &self,
        cost_entry_id: Option<&str>,
        sales_order_id: Option<&str>,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>>;
}

impl CostAllocationReadScopeExt for persistence_core::Repository<'_, CostAllocation> {
    async fn scope_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        mongo_ops::find_many(
            &self.collection(),
            doc! { "deleted_at": 0_i64, "cost_entry_id": { "$in": ids } },
            FindOptions::builder().limit((ALLOCATION_LIMIT + 1) as i64).sort(doc! { "id": 1 }).build(),
            executor,
        )
        .await
    }

    async fn scope_allocation_versions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostAllocationScopeVersion>(),
            doc! { "deleted_at": 0_i64, "cost_entry_id": { "$in": ids } },
            allocation_version_options(),
            executor,
        )
        .await
    }

    async fn scope_matching_allocations(
        &self,
        cost_entry_id: Option<&str>,
        sales_order_id: Option<&str>,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>> {
        if entry_ids.is_empty() {
            return Ok(vec![]);
        }
        let mut filter = matching_allocation_filter(cost_entry_id, sales_order_id);
        filter.insert("$and", vec![doc! { "cost_entry_id": { "$in": entry_ids } }]);
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostAllocationScopeVersion>(),
            filter,
            allocation_version_options(),
            executor,
        )
        .await
    }
}

/// 业务筛选使用交集，软删除条件始终有效。
fn matching_allocation_filter(cost_entry_id: Option<&str>, sales_order_id: Option<&str>) -> Document {
    let mut filter = doc! { "deleted_at": 0_i64 };
    if let Some(id) = cost_entry_id {
        filter.insert("cost_entry_id", id);
    }
    if let Some(id) = sales_order_id {
        filter.insert("sales_order_id", id);
    }
    filter
}

/// 重验只读取身份、版本、成本引用与真实销售来源。
fn allocation_version_options() -> FindOptions {
    FindOptions::builder()
        .limit((ALLOCATION_LIMIT + 1) as i64)
        .sort(doc! { "id": 1 })
        .projection(doc! { "id": 1, "version": 1, "cost_entry_id": 1, "sales_order_id": 1 })
        .build()
}

/// 所有实际分配引用均保留；缺失引用不能被过滤后伪装成零来源成本。
fn whole_source_stages() -> Vec<Document> {
    vec![
        doc! { "$lookup": {
            "from": super::COST_ALLOCATIONS, "let": { "cost_id": "$id" }, "pipeline": [
                { "$match": { "deleted_at": 0, "$expr": { "$eq": ["$cost_entry_id", "$$cost_id"] } } },
                { "$project": { "_id": 0, "sales_order_id": 1 } }
            ], "as": "__cost_whole_allocations"
        } },
        doc! { "$set": {
            "__funds_sales_ids": { "$map": { "input": "$__cost_whole_allocations", "as": "line", "in": { "$ifNull": ["$$line.sales_order_id", null] } } },
            "__funds_purchase_ids": [], "__funds_settlement_ids": [],
            "__funds_sources_valid": { "$allElementsTrue": [{ "$map": {
                "input": "$__cost_whole_allocations", "as": "line", "in": { "$and": [
                    { "$ne": [{ "$ifNull": ["$$line.sales_order_id", null] }, null] },
                    { "$ne": ["$$line.sales_order_id", ""] }
                ] }
            } }] }
        } },
        doc! { "$unset": "__cost_whole_allocations" },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_allocations_intersect_sales_and_cost_filters() {
        assert_eq!(
            matching_allocation_filter(Some("cost-a"), Some("sales-a")),
            doc! { "deleted_at": 0_i64, "cost_entry_id": "cost-a", "sales_order_id": "sales-a" }
        );
        assert_eq!(
            matching_allocation_filter(None, Some("sales-a")),
            doc! { "deleted_at": 0_i64, "sales_order_id": "sales-a" }
        );
        assert_eq!(matching_allocation_filter(None, None), doc! { "deleted_at": 0_i64 });
    }
}

#[cfg(test)]
mod membership_tests {
    use erp_identity::entity::authorization::QueryModelKey;
    use mongodb::bson::Bson;

    use super::*;

    #[test]
    fn id_only_existence_is_accepted_as_membership_and_rejects_amount_or_wrong_collection() {
        let mut model = QueryModel {
            key: QueryModelKey::parse("cost_membership").unwrap(),
            resource: "cost_entry".into(),
            collection: "cost_entries".into(),
            id_field: "id".into(),
            output: QueryOutput::Existence { fields: ["id".into()].into() },
            facts: Default::default(),
        };
        ensure_candidate_model(&model, "cost_entries").unwrap();
        assert!(ensure_candidate_model(&model, "other_entries").is_err());
        model.output = QueryOutput::Existence { fields: ["id".into(), "amount".into()].into() };
        assert!(ensure_candidate_model(&model, "cost_entries").is_err());
    }

    #[test]
    fn allocation_membership_uses_cost_roots_without_accepting_other_resources() {
        let mut model = QueryModel {
            key: QueryModelKey::parse("funds_cost_allocation").unwrap(),
            resource: "cost_allocation".into(),
            collection: "cost_entries".into(),
            id_field: "id".into(),
            output: QueryOutput::Existence { fields: ["id".into()].into() },
            facts: Default::default(),
        };
        ensure_candidate_model(&model, "cost_entries").unwrap();
        model.resource = "invoice".into();
        assert!(ensure_candidate_model(&model, "cost_entries").is_err());
        model.resource = "cost_allocation".into();
        model.collection = "cost_allocations".into();
        assert!(ensure_candidate_model(&model, "cost_entries").is_err());
        model.collection = "cost_entries".into();
        model.id_field = "cost_entry_id".into();
        assert!(ensure_candidate_model(&model, "cost_entries").is_err());
    }

    #[test]
    fn same_root_semijoin_keeps_share_or_whole_membership_before_outer_limit() {
        let shares = [doc! { "$match": { "share_owner": "actor" } }];
        let whole = [doc! { "$match": { "ledger": true } }];
        let stages = membership_stages("cost_entries", &shares, &whole);
        let lookup = stages[0].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_document("let").unwrap(), &doc! { "root": "$id" });
        let pipeline = lookup.get_array("pipeline").unwrap();
        assert_eq!(pipeline[0], Bson::Document(doc! { "$match": { "$expr": { "$eq": ["$id", "$$root"] } } }));
        let facet = pipeline[1].as_document().unwrap().get_document("$facet").unwrap();
        for (name, expected) in [("shares", &shares[0]), ("whole", &whole[0])] {
            let branch = facet.get_array(name).unwrap();
            assert_eq!(branch[0], Bson::Document(expected.clone()));
            assert_eq!(branch.last().unwrap(), &Bson::Document(doc! { "$limit": 1 }));
        }
        assert_eq!(
            pipeline[2],
            Bson::Document(doc! { "$match": { "$expr": { "$or": [
                { "$gt": [{ "$size": "$shares" }, 0] }, { "$gt": [{ "$size": "$whole" }, 0] }
            ] } } })
        );
        assert_eq!(stages[1], doc! { "$match": { "__cost_authorized_root.0": { "$exists": true } } });
        assert_eq!(stages[2], doc! { "$unset": "__cost_authorized_root" });
    }
}
