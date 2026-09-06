use std::collections::{HashMap, HashSet};

use entities::supplier::{SupplierAccount, SupplierAccountStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{PartyId, SupplierAccountId};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::extensions::PartyExt;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::{SupplierRepository, SUPPLIER_ACCOUNTS};
use persistence_core::insert_literal_regex_filter;
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};

/// 供应商角色列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierAccountRow {
    /// 实体主键。
    pub id: String,
    /// 共用企业主体 ID。
    pub party_id: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 默认结算条件引用。
    pub default_payment_term_id: Option<String>,
    /// 当前商务结算版本 ID。
    pub current_commercial_profile_revision_id: Option<String>,
    /// 启停状态。
    pub status: SupplierAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商编号窄投影行。
#[derive(Debug, Clone, Deserialize)]
struct SupplierNumberRow {
    /// 供应商稳定 ID。
    id: String,
    /// 供应商编号。
    supplier_no: String,
}

/// 供应商账号到企业主体的最小关联行。
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct SupplierPartyRefRow {
    /// 供应商账号稳定 ID。
    id: String,
    /// 供应商账号所属企业主体 ID。
    party_id: String,
}

/// 供应商账号业务主键重复审计行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SupplierAccountIdDuplicate {
    /// 重复的业务主键。
    pub id: String,
    /// 该主键在集合中的出现次数。
    pub count: i64,
}

/// 供应商角色列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierAccountFilter {
    /// 供应商编号模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 共用企业主体 ID（精确匹配）；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 共用主体 ID 集合；用于将主体名称命中并入供应商编号搜索。
    pub party_ids: Option<Vec<PartyId>>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<SupplierAccountStatus>,
    /// 必须命中的供应商角色 ID 集合；空集合表示无匹配结果。
    pub supplier_ids: Option<Vec<SupplierAccountId>>,
    /// 必须排除的供应商角色 ID 集合。
    pub excluded_supplier_ids: Option<Vec<SupplierAccountId>>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        match (self.keyword.as_deref(), self.party_ids.as_ref()) {
            (Some(keyword), Some(party_ids)) if !party_ids.is_empty() => {
                let ids: Vec<String> = party_ids.iter().map(ToString::to_string).collect();
                filter.insert(
                    "$or",
                    vec![
                        doc! { "supplier_no": { "$regex": regex::escape(keyword), "$options": "i" } },
                        doc! { "party_id": { "$in": ids } },
                    ],
                );
            }
            (Some(keyword), _) => {
                insert_literal_regex_filter(&mut filter, "supplier_no", Some(keyword));
            }
            (None, _) => {}
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        insert_supplier_id_constraints(
            &mut filter,
            self.supplier_ids.as_deref(),
            self.excluded_supplier_ids.as_deref(),
        );
        filter
    }
}

/// 将供应商候选与排除集合写入账户列表查询条件。
///
/// # 参数
/// * `filter` - 待补充的 MongoDB 查询条件
/// * `supplier_ids` - 必须命中的供应商候选集合；`None` 表示不限制
/// * `excluded_supplier_ids` - 必须排除的供应商集合；`None` 表示不排除
///
/// # 返回
/// 无返回值；查询条件在原地更新。
fn insert_supplier_id_constraints(
    filter: &mut Document,
    supplier_ids: Option<&[SupplierAccountId]>,
    excluded_supplier_ids: Option<&[SupplierAccountId]>,
) {
    let Some(supplier_ids) = supplier_ids else {
        if let Some(excluded_supplier_ids) = excluded_supplier_ids {
            filter.insert("id", doc! { "$nin": supplier_id_strings(excluded_supplier_ids) });
        }
        return;
    };
    let mut id_filter = doc! { "$in": supplier_id_strings(supplier_ids) };
    if let Some(excluded_supplier_ids) = excluded_supplier_ids {
        id_filter.insert("$nin", supplier_id_strings(excluded_supplier_ids));
    }
    filter.insert("id", id_filter);
}

/// 转换供应商角色 ID，供 MongoDB 集合条件使用。
///
/// # 参数
/// * `ids` - 强类型供应商角色 ID 集合
///
/// # 返回
/// 返回保持输入顺序的字符串 ID 集合。
fn supplier_id_strings(ids: &[SupplierAccountId]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

impl Pagination for SupplierAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierAccount> {
    /// 批量读取未删除供应商的稳定 ID 与供应商编号。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商 ID 集合；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回实际存在的供应商编号映射；停用但未删除供应商仍保留编号，软删除或缺失不补行。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn supplier_numbers_by_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let collection = self.collection().clone_with_type::<SupplierNumberRow>();
        let rows = mongo_ops::find_many(
            &collection,
            doc! {
                "id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! { "id": 1, "supplier_no": 1 })
                .build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| (row.id, row.supplier_no)).collect())
    }

    /// 按供应商角色 ID 集合批量读取活跃账户。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商角色 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的供应商角色；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_accounts_by_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccount>> {
        if supplier_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 分页检索供应商角色列表（投影查询）。
    ///
    /// 只返回 [`SupplierAccountRow`] 所需的列表字段，不加载整文档；排序字段
    /// 经仓储白名单校验（`created_at`/`supplier_no`/`status`），非法字段回落
    /// 默认 `created_at`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_supplier_accounts(
        &self,
        filter: &SupplierAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "supplier_no", "status"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按共用企业主体查找供应商角色（一个主体至多一个供应商角色，由
    /// `uk_supplier_accounts_party` 保证）。
    ///
    /// # 参数
    /// * `party_id` - 共用企业主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除供应商角色；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        self.find_one(doc! { "party_id": party_id.to_string() }, executor)
            .await
    }
}

impl<'a> SupplierRepository<'a> {
    /// 按稳定 ID 读取未删除供应商角色账号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配供应商角色；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn account(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        Repository::new(self.db, SUPPLIER_ACCOUNTS)
            .find_by_id(supplier_id.as_ref(), executor)
            .await
    }

    /// 按供应商账号 ID 批量读取当前主体修订的法定名称。
    ///
    /// 查询仅返回未删除供应商账号、未删除 Party 及其未删除当前修订形成的
    /// `账号 ID -> 法定名称` 投影。缺失任一关联或缺少当前修订指针时不生成键；
    /// 法定名称按持久化原值返回，不在仓储层执行空白回退等业务决策。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商账号 ID；允许重复，空集合直接返回空映射
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回供应商账号 ID 到当前法定名称的映射。
    ///
    /// # 错误
    /// 当任一批量查询或投影反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只查询本域 `supplier_accounts` 集合，主体法定名称经主体域属主访问器组装。
    pub async fn current_legal_names_by_account_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let supplier_ids = unique_strings(supplier_ids.iter().map(ToString::to_string));
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let suppliers = self.supplier_party_refs(&supplier_ids, executor).await?;
        let party_ids = unique_strings(suppliers.iter().map(|row| row.party_id.clone()))
            .into_iter()
            .map(PartyId::new)
            .collect::<Vec<_>>();
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let party_names = self
            .db
            .party()
            .current_legal_names_by_party_ids(&party_ids, executor)
            .await?;
        Ok(suppliers
            .into_iter()
            .filter_map(|row| {
                party_names
                    .get(&row.party_id)
                    .map(|legal_name| (row.id, legal_name.clone()))
            })
            .collect())
    }

    /// 批量读取未删除供应商账号到企业主体的关联。
    ///
    /// # 参数
    /// * `supplier_ids` - 已去重的供应商账号 ID
    /// * `executor` - 调用方提供的数据访问执行器
    ///
    /// # 返回
    /// 返回全部命中的账号与主体最小关联行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或投影反序列化失败时返回错误。
    async fn supplier_party_refs(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPartyRefRow>> {
        mongo_ops::find_many(
            &self.db.collection::<SupplierPartyRefRow>(SUPPLIER_ACCOUNTS),
            active_ids_filter(supplier_ids),
            FindOptions::builder()
                .projection(supplier_party_ref_projection())
                .build(),
            executor,
        )
        .await
    }

    /// 审计 `supplier_accounts` 集合中重复的业务主键 `id`。
    ///
    /// 部署 `uk_supplier_accounts_id` 唯一索引前的门禁查询：按 `id` 分组统计
    /// 出现次数并返回全部出现超过一次的主键，供部署在建索引前失败关闭。
    /// 审计覆盖全部文档（含已软删除）：身份唯一索引是全局的，软删除后仍保留
    /// 身份，删除态重复同样会阻断迁移。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中；本方法只读，
    ///   不开启或提交事务
    ///
    /// # 返回
    /// 按 `id` 字典序排列的重复主键及出现次数；无重复时返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 聚合或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只查询本域 `supplier_accounts` 集合；仅返回审计事实，不执行清理或建索引。
    pub async fn duplicate_supplier_account_ids(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountIdDuplicate>> {
        aggregate_id_duplicates(&self.db.collection::<Document>(SUPPLIER_ACCOUNTS), executor).await
    }
}

/// 构建排序文档（仓储白名单）。
///
/// `sort_by` 不在 `allowed` 白名单内时回落默认 `created_at`，禁止透传任意
/// 字段名（P2 §2.3）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|candidate| allowed.contains(candidate))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 供应商角色列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_account_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "supplier_no": 1,
        "default_payment_term_id": 1,
        "current_commercial_profile_revision_id": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 按首次出现顺序去重字符串集合。
///
/// # 参数
/// * `values` - 允许包含重复值的字符串迭代器
///
/// # 返回
/// 返回保留首次出现顺序的唯一字符串集合。
fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value);
        }
    }
    unique
}

/// 构建未删除稳定对象的批量 ID 查询条件。
///
/// # 参数
/// * `ids` - 已去重的稳定对象 ID
///
/// # 返回
/// 返回同时限定 ID 集合与软删除标记的查询文档。
fn active_ids_filter(ids: &[String]) -> Document {
    doc! {
        "id": { "$in": ids },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回供应商账号到主体关联的最小字段投影。
///
/// # 返回
/// 返回排除 MongoDB `_id` 且仅保留账号与主体 ID 的投影文档。
fn supplier_party_ref_projection() -> Document {
    doc! { "_id": 0, "id": 1, "party_id": 1 }
}

/// 执行业务主键重复审计聚合，复用调用方执行器的会话语义。
///
/// # 参数
/// * `collection` - 供应商账号集合句柄
/// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
///
/// # 返回
/// 按 `id` 字典序排列的重复主键及出现次数。
///
/// # 错误
/// 当 MongoDB 聚合或游标读取失败时返回错误。
async fn aggregate_id_duplicates(
    collection: &mongodb::Collection<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierAccountIdDuplicate>> {
    let pipeline = supplier_account_id_duplicate_pipeline();
    let rows = match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<SupplierAccountIdDuplicate>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<SupplierAccountIdDuplicate>()
                .await?
                .try_collect::<Vec<_>>()
                .await
        }
    }
    .map_err(persistence_core::Error::from)?;
    Ok(rows)
}

/// 返回业务主键重复审计的聚合管道，供审计执行与测试共用。
///
/// 按 `id` 分组计数，仅保留出现超过一次的主键并按 `id` 字典序稳定排列；
/// 不附加软删除过滤，全局唯一索引同样约束已软删除文档。
///
/// # 返回
/// 与 [`SupplierRepository::duplicate_supplier_account_ids`] 相同的聚合管道。
fn supplier_account_id_duplicate_pipeline() -> Vec<Document> {
    vec![
        doc! { "$group": { "_id": "$id", "count": { "$sum": 1 } } },
        doc! { "$match": { "count": { "$gt": 1 } } },
        doc! { "$sort": { "_id": 1 } },
        doc! { "$project": { "_id": 0, "id": "$_id", "count": 1 } },
    ]
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, SupplierAccountFilter};
    use entities::supplier::SupplierAccountStatus;
    use mongodb::bson::doc;

    #[test]
    fn account_filter_applies_candidate_and_excluded_supplier_ids() {
        let filter = SupplierAccountFilter {
            keyword: None,
            party_id: None,
            party_ids: None,
            status: Some(SupplierAccountStatus::Active),
            supplier_ids: Some(vec![erp_core::ids::SupplierAccountId::new("supplier-1")]),
            excluded_supplier_ids: Some(vec![erp_core::ids::SupplierAccountId::new("supplier-2")]),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let ids = document.get_document("id").unwrap();
        assert_eq!(ids.get_array("$in").unwrap().len(), 1);
        assert_eq!(ids.get_array("$nin").unwrap().len(), 1);
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted() {
        assert_eq!(
            sort_doc(Some("revised_at"), false, &["created_at", "supplier_no"]),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("supplier_no"), true, &["created_at", "supplier_no"]),
            doc! { "supplier_no": 1 }
        );
    }

    #[test]
    fn duplicate_audit_pipeline_groups_and_reports_repeated_ids() {
        let pipeline = super::supplier_account_id_duplicate_pipeline();

        assert_eq!(pipeline.len(), 4);
        let rendered = format!("{pipeline:?}");
        assert!(rendered.contains("$group"));
        assert!(rendered.contains("$match"));
        assert!(rendered.contains("$sort"));
        assert!(rendered.contains("$project"));
        assert_eq!(
            pipeline[1]
                .get_document("$match")
                .unwrap()
                .get_document("count")
                .unwrap(),
            &doc! { "$gt": 1 }
        );
    }
}

/// PROC-R10 供应商业务主键索引的真实 MongoDB 验收（隔离库，Quality 单独执行）。
#[cfg(test)]
mod proc_r10_mongo_tests {
    use mongodb::bson::{doc, Document};
    use test_support::{require_mongo, TestDb};

    use super::{SupplierAccountIdDuplicate, SUPPLIER_ACCOUNTS};
    use crate::{ensure_indexes, SupplierExt};
    use persistence_core::NoTransaction;

    /// 插入仅携带索引相关字段的供应商账号原始文档。
    ///
    /// # 参数
    /// * `db` - 隔离测试库
    /// * `id` - 业务主键（可故意重复）
    /// * `party_id` - 所属主体（保持唯一，避免干扰其他唯一索引）
    /// * `supplier_no` - 供应商编号（保持唯一，避免干扰其他唯一索引）
    ///
    /// # 错误
    /// 写入失败时 panic。
    async fn insert_raw_supplier_account(
        db: &mongodb::Database,
        id: &str,
        party_id: &str,
        supplier_no: &str,
    ) {
        db.collection::<Document>(SUPPLIER_ACCOUNTS)
            .insert_one(doc! {
                "id": id,
                "party_id": party_id,
                "supplier_no": supplier_no,
                "deleted_at": 0_i64,
            })
            .await
            .expect("原始供应商账号写入失败");
    }

    /// 重复 `id` 必须先被审计报出，再拒绝索引迁移并输出冲突索引诊断。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 审计命中重复主键且迁移失败关闭时通过。
    ///
    /// # 错误
    /// 审计漏报或迁移未拒绝时测试失败。
    ///
    /// # 约束
    /// 仅验证 `supplier_accounts` 集合；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn duplicate_supplier_account_ids_are_audited_and_refuse_migration() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_r10_supplier_id_dup")
                .await
                .expect("测试数据库创建失败");
            insert_raw_supplier_account(fixture.db(), "dup-1", "pty-a", "SUP-A").await;
            insert_raw_supplier_account(fixture.db(), "dup-1", "pty-b", "SUP-B").await;
            insert_raw_supplier_account(fixture.db(), "sup-ok", "pty-c", "SUP-C").await;

            let duplicates = fixture
                .db()
                .supplier()
                .duplicate_supplier_account_ids(&mut NoTransaction)
                .await
                .expect("重复审计查询失败");
            assert_eq!(
                duplicates,
                vec![SupplierAccountIdDuplicate {
                    id: "dup-1".to_string(),
                    count: 2
                }],
                "部署前审计必须报出重复 id"
            );

            let err = ensure_indexes(fixture.db())
                .await
                .expect_err("重复 id 必须拒绝建索引");
            let rendered = format!("{err:?}");
            assert!(
                rendered.contains("uk_supplier_accounts_id"),
                "诊断必须包含冲突索引名：{rendered}"
            );
        });
    }

    /// `id $in` 批量查询的执行计划必须命中唯一索引且无集合扫描。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// `explain` 命中 `uk_supplier_accounts_id` 的 `IXSCAN` 且无 `COLLSCAN` 时通过。
    ///
    /// # 错误
    /// 索引未命中或出现集合扫描时测试失败。
    ///
    /// # 约束
    /// 不使用 `hint`；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_id_in_queries_use_unique_id_index() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_r10_supplier_id_explain")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            insert_raw_supplier_account(fixture.db(), "sup-1", "pty-1", "SUP-1").await;

            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": SUPPLIER_ACCOUNTS,
                        "filter": {
                            "id": { "$in": ["sup-1", "sup-missing"] },
                            "deleted_at": 0_i64,
                        },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("供应商 id 查询 explain 失败");
            let rendered = format!("{explain:?}");
            assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
            assert!(
                rendered.contains("uk_supplier_accounts_id"),
                "explain 未命中 uk_supplier_accounts_id：{rendered}"
            );
            assert!(
                !rendered.contains("COLLSCAN"),
                "explain 出现 COLLSCAN：{rendered}"
            );
        });
    }
}
