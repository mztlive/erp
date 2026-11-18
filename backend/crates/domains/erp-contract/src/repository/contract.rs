//! 域 D12 `contract` 仓储：contract、contract_revision（页面：W04）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；`contract_revision` 是
//! 不可变修订（数据模型 §4.3），**不提供软删除/恢复方法**（事实类集合约定）。
//! 本文件只补充域特有查询与跨集合多步骤写入入口；集合名常量统一取
//! `ContractExt` 关联常量（单一权威来源，conventions §4.3）。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use crate::entity::contract::{Contract, ContractId, ContractRevision, ContractStatus};
use crate::repository::extensions::ContractExt;
use crate::repository::owned::ContractRepository;

/// `contract` 集合名（单一来源：`ContractExt` 关联常量）。
const CONTRACTS: &str = <mongodb::Database as ContractExt>::CONTRACTS;
/// `contract_revision` 集合名（单一来源：`ContractExt` 关联常量）。
const CONTRACT_REVISIONS: &str = <mongodb::Database as ContractExt>::CONTRACT_REVISIONS;

/// 合同列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRow {
    /// 合同独立负责销售；历史缺失不得由客户归属补写。
    #[serde(default)]
    pub sales_owner_user_id: String,
    /// 实体主键。
    pub id: String,
    /// 合同编号。
    pub contract_no: String,
    /// 客户。
    pub customer_id: String,
    /// 结算主体。
    pub settlement_party_id: String,
    /// 合同状态。
    pub status: ContractStatus,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 更新时间（秒级时间戳）。
    pub updated_at: u64,
}

/// 合同修订号最小投影行。
#[derive(Debug, Deserialize)]
struct ContractRevisionNoRow {
    /// 聚合内修订序号。
    revision_no: u32,
}

/// 合同列表筛选条件。
#[derive(Debug, Clone)]
pub struct ContractFilter {
    /// identity编译的完整授权阶段。
    pub access: Option<super::scope::ContractReadScope>,
    /// 合同编号（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub contract_no: Option<String>,
    /// 单个客户；`None` 表示不按单客户筛选。
    pub customer_id: Option<String>,
    /// 授权客户集合；`None` 表示公司范围，空向量匹配不到任何客户合同。
    pub customer_ids: Option<Vec<String>>,
    /// 目录“我负责”按合同负责或明确共享人员收窄。
    pub assigned_user_id: Option<String>,
    /// 请求中的负责销售组织条件，始终只收窄授权合同。
    pub owner_user_ids: Option<Vec<String>>,
    /// 合同状态；`None` 表示不筛选。
    pub status: Option<ContractStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl ContractFilter {
    /// 返回identity完整授权阶段。
    /// 当前查询阶段。
    /// 不执行I/O。
    pub fn stages(&self) -> Vec<Document> {
        self.access.as_ref().map_or_else(Vec::new, |scope| scope.stages())
    }

    /// 将已授权候选客户与仍存在的来源客户求交，历史参与不得扩张来源边界。
    /// 原地收窄客户条件；空集合保持空结果。
    /// `existing` 为同一执行器按候选合同引用 ID 查询的未删除客户。
    pub fn require_existing_customers(&mut self, existing: Vec<String>) {
        self.customer_ids = Some(
            existing
                .into_iter()
                .filter(|id| self.customer_ids.as_ref().is_none_or(|allowed| allowed.contains(id)))
                .collect(),
        );
    }
}

impl Default for ContractFilter {
    // 初始化合同筛选：第一页、每页 20 条，业务筛选默认留空。
    fn default() -> Self {
        Self {
            access: None,
            contract_no: None,
            customer_id: None,
            customer_ids: None,
            assigned_user_id: None,
            owner_user_ids: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for ContractFilter {
    // 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    // `customer_id` 与 `customer_ids` 同时存在时按交集收窄：请求客户不在集合内时写入空 `$in`，保证分页总数为 0 而不是退回全量合同。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "contract_no", self.contract_no.as_deref());

        match (&self.customer_id, &self.customer_ids) {
            (Some(id), Some(ids)) => {
                filter.insert(
                    "customer_id",
                    doc! {"$in":ids.iter().filter(|value|*value==id).cloned().collect::<Vec<_>>()},
                );
            },
            (Some(id), None) => {
                filter.insert("customer_id", id);
            },
            (None, Some(ids)) => {
                filter.insert("customer_id", doc! {"$in":ids});
            },
            (None, None) => {},
        }

        if let Some(user) = &self.assigned_user_id {
            filter = doc! { "$and": [filter, { "$or": [
                { "sales_owner_user_id": user }, { "shared_sales_user_ids": user }
            ] }] };
        }
        if let Some(ids) = &self.owner_user_ids {
            filter.insert("sales_owner_user_id", doc! { "$in": ids });
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ContractFilter {
    // 向仓储分页传递页码与单页条数。
    // 读取筛选条件上的 `page`，并把 `page_size` 从 `u32` 拓宽为 `u64`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 合同集合上的域特有查询。
#[allow(async_fn_in_trait)]
pub trait ContractRepositoryExt {
    /// 分页检索合同列表（投影查询）。
    /// 只返回 [`ContractRow`] 所需的列表字段，不加载整文档；排序字段由 Service 层白名单校验后传入（api-contract §4）。
    /// 返回当前页投影行与满足筛选条件的总数。
    /// 沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询、游标读取或计数错误。
    async fn search_contracts(
        &self,
        filter: &ContractFilter,
        executor: &mut dyn Executor,
    ) -> crate::Result<PageResult<ContractRow>>;

    /// 查找当前客户范围内指向指定结算主体的生效合同。
    /// 返回任一客户范围内的生效合同；客户集合为空或无匹配时返回 `None`。
    /// `customer_ids` 为当前操作人可参与的客户 ID 集合；`settlement_party_id` 为目标结算主体 ID。
    /// 沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询错误。
    /// 仅查询本仓储拥有的 `contracts` 集合，不访问结算主体或客户集合。
    async fn find_effective_for_settlement_party(
        &self,
        customer_ids: &[String],
        settlement_party_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Contract>>;

    /// 返回匹配条件的未删除对象 ID，供跨域列表在分页前组合筛选。
    /// 未删除且编号匹配的合同 ID。空命中为空集合，不扩大范围。非字符串 ID 略过。
    /// `keyword` 为合同编号的字面量关键词；沿用调用方执行器 `executor`。
    /// 数据库查询失败时返回错误。
    async fn matching_ids_by_number(&self, keyword: &str, executor: &mut dyn Executor)
    -> Result<Vec<String>>;
}

impl ContractRepositoryExt for persistence_core::Repository<'_, Contract> {
    async fn search_contracts(
        &self,
        filter: &ContractFilter,
        executor: &mut dyn Executor,
    ) -> crate::Result<PageResult<ContractRow>> {
        if let Some(scope) = &filter.access {
            scope.ensure_current(executor)?;
        }

        let result: persistence_core::Result<_> = async {
            let options = FindOptions::builder()
                .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
                .skip(filter.skip())
                .limit(filter.limit())
                .projection(contract_projection())
                .build();
            let collection = self.collection().clone_with_type::<ContractRow>();
            let items = persistence_core::aggregate::find(
                &collection,
                filter.to_doc(),
                options,
                filter.stages(),
                executor,
            )
            .await?;
            let total = persistence_core::aggregate::count(
                &self.collection(),
                filter.to_doc(),
                filter.stages(),
                executor,
            )
            .await?;
            Ok(PageResult { items, total: total as i64 })
        }
        .await;
        let result = result?;

        if let Some(scope) = &filter.access {
            scope.ensure_current(executor)?;
        }
        Ok(result)
    }

    async fn find_effective_for_settlement_party(
        &self,
        customer_ids: &[String],
        settlement_party_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Contract>> {
        if customer_ids.is_empty() {
            return Ok(None);
        }
        self.find_one(
            doc! {
                "customer_id": { "$in": customer_ids },
                "settlement_party_id": settlement_party_id,
                "status": ContractStatus::Effective.as_str(),
            },
            executor,
        )
        .await
    }

    async fn matching_ids_by_number(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "contract_no", Some(keyword));
        let collection = self.collection();
        Ok(mongo_ops::distinct(&collection, "id", filter, executor)
            .await?
            .into_iter()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .collect())
    }
}

/// 合同修订集合上的域特有查询。
#[allow(async_fn_in_trait)]
pub trait ContractRevisionRepositoryExt {
    /// 按修订 ID 集合批量读取不可变合同版本。
    /// 命中的版本。空 ID 集合返回空向量，未命中的 ID 不出现。
    /// `revision_ids` 为修订主键。空集合不查询；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询错误。
    async fn find_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractRevision>>;

    /// 列出合同的全部版本（新版本在前）。
    /// 返回按版本号倒序的合同版本列表。
    /// `contract_id` 为所属合同；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
    async fn list_by_contract(
        &self,
        contract_id: &ContractId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractRevision>>;

    /// 读取指定合同的历史最大修订序号。
    /// 查询只读取 `revision_no` 并限制一条；修订号分配与溢出校验由实体负责。
    /// 返回历史最大修订序号；没有历史修订时返回 `None`。
    /// `contract_id` 为所属合同；沿用调用方执行器 `executor`。
    /// 当 MongoDB 查询或投影反序列化失败时返回错误。
    async fn latest_revision_no(
        &self,
        contract_id: &ContractId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>>;
}

impl ContractRevisionRepositoryExt for persistence_core::Repository<'_, ContractRevision> {
    async fn find_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": revision_ids } }, executor).await
    }

    async fn list_by_contract(
        &self,
        contract_id: &ContractId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractRevision>> {
        self.find_many_sorted(
            doc! { "contract_id": contract_id.to_string() },
            doc! { "revision_no": -1 },
            executor,
        )
        .await
    }

    async fn latest_revision_no(
        &self,
        contract_id: &ContractId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<ContractRevisionNoRow>(),
            latest_contract_revision_filter(contract_id),
            latest_contract_revision_options(),
            executor,
        )
        .await?;
        Ok(contract_revision_no_from_rows(rows))
    }
}

/// D12 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 合同归档是「插入不可变版本 + 切换当前版本指针」的两步写入；单一集合 CRUD
/// 使用 [`Repository`] 基类。本类型由 `ContractExt::contract()` 访问。
pub struct ContractDomainRepository<'a> {
    pub(super) db: &'a Database,
}

impl<'a> ContractDomainRepository<'a> {
    /// 创建域专用仓储。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 首次归档合同（创建合同 + 首个不可变版本 + 绑定当前版本指针）。
    /// 依次写入 `contract`、`contract_revision` 并 CAS 更新合同当前版本指针，保证「合同身份 + 首个不可变版本 + PDF 关联」原子可见（数据模型 §6.4）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时各步自动提交，中途失败会留下没有版本的合同半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    /// 无返回值。合同与首个不可变版本写入完成。
    /// `contract` 为待写入的合同（成功后内存中绑定首个版本指针并递增版本）；`revision` 为首个不可变合同版本。
    /// 沿用调用方当前事务的执行器 `executor`。
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）、乐观锁冲突或 MongoDB 写入失败时返回错误。
    pub async fn create_contract_with_revision(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<Contract>(CONTRACTS), contract, executor).await?;
        self.attach_revision(contract, revision, executor).await
    }

    /// 归档合同新版本（插入版本 + 绑定当前版本指针）。
    /// 依次写入 `contract_revision` 并 CAS 更新合同当前版本指针；同一合同允许追加更高序号的版本（数据模型 §4.3）。**必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，中途失败会留下没有绑定指针的版本；Service 必须通过 `with_transaction` 传入事务会话。
    /// 无返回值。新版本写入完成并已绑定当前版本指针。
    /// `contract` 为待绑定新版本的合同（成功后内存中切换版本指针并递增版本）；`revision` 为新不可变合同版本。
    /// 沿用调用方当前事务的执行器 `executor`。
    /// 当唯一索引冲突、乐观锁冲突或 MongoDB 写入失败时返回错误。
    pub async fn archive_contract_revision(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.attach_revision(contract, revision, executor).await
    }

    // 落不可变版本并切换合同当前版本指针（两方法共用后半段）。
    // `contract` 为待绑定版本的合同（成功后内存中切换版本指针并递增版本）；`revision` 为新不可变合同版本。
    // 沿用调用方当前事务的执行器 `executor`。
    // 当唯一索引冲突、乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn attach_revision(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<ContractRevision>(CONTRACT_REVISIONS),
            revision,
            executor,
        )
        .await?;
        contract.attach_revision(&revision.base.id, contract.stable.updated_by.clone());
        ContractRepository::new(self.db, CONTRACTS).update(contract, executor).await
    }
}

// 构建排序文档。
// `sort_by` 为排序字段；`None` 时默认 `created_at`；`sort_ascending` 为升序为 `true`，降序为 `false`。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction }
}

// 合同列表投影字段。
fn contract_projection() -> Document {
    doc! {
        "sales_owner_user_id": 1,
        "id": 1,
        "contract_no": 1,
        "customer_id": 1,
        "settlement_party_id": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
        "updated_at": 1,
    }
}

// 构建合同最大修订号查询条件。
fn latest_contract_revision_filter(contract_id: &ContractId) -> Document {
    doc! {
        "contract_id": contract_id.to_string(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

// 构建合同最大修订号的最小投影与有界排序。
fn latest_contract_revision_options() -> FindOptions {
    FindOptions::builder()
        .sort(doc! { "revision_no": -1 })
        .limit(1)
        .projection(doc! { "revision_no": 1, "_id": 0 })
        .build()
}

// 从已按修订号倒序返回的零或一条投影中读取最大修订号。
fn contract_revision_no_from_rows(rows: Vec<ContractRevisionNoRow>) -> Option<u32> {
    rows.into_iter().next().map(|row| row.revision_no)
}

#[cfg(test)]
mod tests {
    use erp_core::ids::ContractId;
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::{
        ContractFilter, ContractRevisionNoRow, contract_revision_no_from_rows,
        latest_contract_revision_filter, latest_contract_revision_options, sort_doc,
    };

    #[test]
    fn contract_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ContractFilter {
            contract_no: Some("HT-2026".to_string()),
            customer_id: Some("cust-1".to_string()),
            status: Some(crate::entity::contract::ContractStatus::Effective),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_document("contract_no").unwrap().get_str("$regex").unwrap(), r"HT\-2026");
        assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(document.get_str("status").unwrap(), "EFFECTIVE");
    }

    #[test]
    fn contract_filter_without_optional_fields_only_applies_deleted_filter() {
        let filter = ContractFilter::default();

        assert_eq!(filter.to_doc(), doc! { "deleted_at": 0_i64 });
    }

    #[test]
    fn contract_filter_customer_ids_use_in_and_intersect_with_single_id() {
        let in_scope = ContractFilter {
            customer_ids: Some(vec!["cust-1".to_string(), "cust-2".to_string()]),
            ..Default::default()
        };
        assert_eq!(
            in_scope.to_doc().get_document("customer_id").unwrap(),
            &doc! { "$in": ["cust-1", "cust-2"] }
        );

        let hit = ContractFilter {
            customer_id: Some("cust-2".to_string()),
            customer_ids: Some(vec!["cust-1".to_string(), "cust-2".to_string()]),
            ..in_scope.clone()
        };
        assert_eq!(hit.to_doc().get_document("customer_id").unwrap(), &doc! { "$in": ["cust-2"] });

        let miss = ContractFilter {
            customer_id: Some("cust-9".to_string()),
            customer_ids: Some(vec!["cust-1".to_string()]),
            ..in_scope.clone()
        };
        let miss = miss.to_doc();
        assert!(miss.get_document("customer_id").unwrap().get_array("$in").unwrap().is_empty());

        let empty_candidates = ContractFilter {
            customer_id: Some("cust-1".into()),
            customer_ids: Some(Vec::new()),
            ..Default::default()
        }
        .to_doc();
        assert!(empty_candidates.get_document("customer_id").unwrap().get_array("$in").unwrap().is_empty());
    }

    #[test]
    fn sort_doc_defaults_to_created_at_descending() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("contract_no"), true), doc! { "contract_no": 1 });
    }

    #[test]
    fn latest_revision_query_is_minimal_bounded_and_reads_empty_history() {
        assert_eq!(
            latest_contract_revision_filter(&ContractId::new("contract-1")),
            doc! { "contract_id": "contract-1", "deleted_at": 0_i64 }
        );
        let options = latest_contract_revision_options();
        assert_eq!(options.sort, Some(doc! { "revision_no": -1 }));
        assert_eq!(options.limit, Some(1));
        assert_eq!(options.projection, Some(doc! { "revision_no": 1, "_id": 0 }));
        assert_eq!(contract_revision_no_from_rows(Vec::new()), None);
        assert_eq!(contract_revision_no_from_rows(vec![ContractRevisionNoRow { revision_no: 7 }]), Some(7));
    }

    #[test]
    fn contract_and_revision_bson_keep_identity_status_and_snapshot_fields() {
        use erp_core::common::time::BusinessDate;
        use erp_core::ids::{ContractId, ContractRevisionId, CustomerAccountId, FileAssetId, PartyId};

        use crate::entity::contract::{
            ArchiveSource, Contract, ContractData, ContractRevision, ContractRevisionData, ContractStatus,
        };

        let contract = Contract::new(
            ContractId::new("c-1"),
            ContractData {
                contract_no: "HT-2026-0088".to_string(),
                customer_id: CustomerAccountId::new("cust-1"),
                settlement_party_id: PartyId::new("party-1"),
            },
            "admin-1",
        )
        .unwrap();
        let document = mongodb::bson::serialize_to_document(&contract).unwrap();
        assert_eq!(document.get_str("id").unwrap(), "c-1");
        assert_eq!(document.get_str("contract_no").unwrap(), "HT-2026-0088");
        assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(document.get_str("settlement_party_id").unwrap(), "party-1");
        assert_eq!(document.get_str("status").unwrap(), "EFFECTIVE");
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(contract.stable.status, ContractStatus::Effective);

        let revision = ContractRevision::new(
            ContractRevisionId::new("rev-1"),
            ContractId::new("c-1"),
            1,
            ContractRevisionData {
                contract_no: "HT-2026-0088".to_string(),
                customer_name: "东方企业".to_string(),
                contract_pdf_file_id: FileAssetId::new("file-1"),
                archive_source: ArchiveSource::ContractCenter,
                settlement_party_id: PartyId::new("party-1"),
                settlement_party_name: "集团结算中心".to_string(),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结 30 天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "6".to_string(),
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
                signed_at: BusinessDate::from_ymd(2025, 12, 20).unwrap(),
            },
        )
        .unwrap();
        let revision_doc = mongodb::bson::serialize_to_document(&revision).unwrap();
        assert_eq!(revision_doc.get_str("id").unwrap(), "rev-1");
        assert_eq!(revision_doc.get_str("contract_id").unwrap(), "c-1");
        assert_eq!(revision_doc.get_i64("revision_no").unwrap(), 1);
        assert_eq!(revision_doc.get_str("contract_pdf_file_id").unwrap(), "file-1");
        assert_eq!(revision_doc.get_str("archive_source").unwrap(), "CONTRACT_CENTER");
        assert_eq!(
            revision_doc.get_document("customer_snapshot").unwrap().get_str("customer_name").unwrap(),
            "东方企业"
        );
        assert_eq!(
            revision_doc.get_document("payment_term_snapshot").unwrap().get_str("payment_term_code").unwrap(),
            "NET30"
        );

        let contract_roundtrip: Contract =
            mongodb::bson::deserialize_from_document(document.clone()).unwrap();
        assert_eq!(contract_roundtrip, contract);
        let revision_roundtrip: ContractRevision =
            mongodb::bson::deserialize_from_document(revision_doc).unwrap();
        assert_eq!(revision_roundtrip, revision);
    }

    #[test]
    fn source_existence_limits_company_without_requiring_an_owner() {
        let mut company = ContractFilter::default();
        company.require_existing_customers(vec!["existing-unassigned-customer".into()]);
        assert_eq!(company.customer_ids, Some(vec!["existing-unassigned-customer".into()]));

        let mut restricted =
            ContractFilter { customer_ids: Some(vec!["allowed".into()]), ..Default::default() };
        restricted.require_existing_customers(vec!["outside".into()]);
        assert_eq!(restricted.customer_ids, Some(Vec::new()));
        let mut missing = ContractFilter::default();
        missing.require_existing_customers(Vec::new());
        assert_eq!(missing.customer_ids, Some(Vec::new()));
    }
}
