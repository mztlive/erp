//! 域 D12 `contract` 仓储：contract、contract_revision（页面：W04）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；`contract_revision` 是
//! 不可变修订（数据模型 §4.3），**不提供软删除/恢复方法**（事实类集合约定）。
//! 本文件只补充域特有查询与跨集合多步骤写入入口；集合名常量统一取
//! `ContractExt` 关联常量（单一权威来源，conventions §4.3）。

use entities::contract::{Contract, ContractId, ContractRevision, ContractStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::ContractExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `contract` 集合名（单一来源：`ContractExt` 关联常量）。
const CONTRACTS: &str = <mongodb::Database as ContractExt>::CONTRACTS;
/// `contract_revision` 集合名（单一来源：`ContractExt` 关联常量）。
const CONTRACT_REVISIONS: &str = <mongodb::Database as ContractExt>::CONTRACT_REVISIONS;

/// 合同列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRow {
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

/// 合同列表筛选条件。
#[derive(Debug, Clone)]
pub struct ContractFilter {
    /// 合同编号（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub contract_no: Option<String>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<String>,
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

impl QueryFilter for ContractFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "contract_no", self.contract_no.as_deref());
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ContractFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Contract> {
    /// 分页检索合同列表（投影查询）。
    ///
    /// 只返回 [`ContractRow`] 所需的列表字段，不加载整文档；排序字段由 Service
    /// 层白名单校验后传入（api-contract §4）。
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
    pub async fn search_contracts(
        &self,
        filter: &ContractFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ContractRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(contract_projection())
            .build();
        let collection = self.collection().clone_with_type::<ContractRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按合同编号查找合同。
    ///
    /// 唯一性由 `uk_contracts_contract_no` 唯一索引保证（软删除后编号不复用）。
    ///
    /// # 参数
    /// * `contract_no` - 合同编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除合同；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_contract_no(
        &self,
        contract_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Contract>> {
        self.find_one_by_field("contract_no", contract_no, executor).await
    }
}

impl<'a> Repository<'a, ContractRevision> {
    /// 按合同与版本号查找合同版本。
    ///
    /// 唯一性由 `uk_contract_revisions_contract_revision` 唯一索引保证。
    ///
    /// # 参数
    /// * `contract_id` - 所属合同
    /// * `revision_no` - 聚合内版本号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的合同版本；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_contract_and_no(
        &self,
        contract_id: &ContractId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ContractRevision>> {
        self.find_one(
            doc! {
                "contract_id": contract_id.to_string(),
                "revision_no": revision_no as i32,
            },
            executor,
        )
        .await
    }

    /// 列出合同的全部版本（新版本在前）。
    ///
    /// # 参数
    /// * `contract_id` - 所属合同
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按版本号倒序的合同版本列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_contract(
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
}

/// D12 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 合同归档是「插入不可变版本 + 切换当前版本指针」的两步写入；单一集合 CRUD
/// 使用 [`Repository`] 基类。本类型由 `ContractExt::contract()` 访问。
pub struct ContractRepository<'a> {
    db: &'a Database,
}

impl<'a> ContractRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 首次归档合同（创建合同 + 首个不可变版本 + 绑定当前版本指针）。
    ///
    /// 依次写入 `contract`、`contract_revision` 并 CAS 更新合同当前版本指针，
    /// 保证「合同身份 + 首个不可变版本 + PDF 关联」原子可见（数据模型 §6.4）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时各
    /// 步自动提交，中途失败会留下没有版本的合同半成品；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `contract` - 待写入的合同（成功后内存中绑定首个版本指针并递增版本）
    /// * `revision` - 首个不可变合同版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、乐观锁冲突或
    /// MongoDB 写入失败时返回错误。
    pub async fn create_contract_with_revision(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<Contract>(CONTRACTS), contract, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<ContractRevision>(CONTRACT_REVISIONS),
            revision,
            executor,
        )
        .await?;
        contract.attach_revision(&revision.base.id, contract.stable.updated_by.clone());
        Repository::new(self.db, CONTRACTS)
            .update(contract, executor)
            .await
    }

    /// 归档合同新版本（插入版本 + 绑定当前版本指针）。
    ///
    /// 依次写入 `contract_revision` 并 CAS 更新合同当前版本指针；同一合同允许
    /// 追加更高序号的版本（数据模型 §4.3）。**必须收到事务执行器**：传入
    /// `NoTransaction` 时两笔写入各自自动提交，中途失败会留下没有绑定指针的
    /// 版本；Service 必须通过 `with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `contract` - 待绑定新版本的合同（成功后内存中切换版本指针并递增版本）
    /// * `revision` - 新不可变合同版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突、乐观锁冲突或 MongoDB 写入失败时返回错误。
    pub async fn archive_contract_revision(
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
        Repository::new(self.db, CONTRACTS)
            .update(contract, executor)
            .await
    }
}

/// 构建排序文档。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction }
}

/// 合同列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn contract_projection() -> Document {
    doc! {
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

#[cfg(test)]
mod tests {
    use super::{sort_doc, ContractFilter, QueryFilter};
    use mongodb::bson::doc;

    #[test]
    fn contract_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ContractFilter {
            contract_no: Some("HT-2026".to_string()),
            customer_id: Some("cust-1".to_string()),
            status: Some(entities::contract::ContractStatus::Effective),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(
            document
                .get_document("contract_no")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"HT\-2026"
        );
        assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(document.get_str("status").unwrap(), "EFFECTIVE");
    }

    #[test]
    fn contract_filter_without_optional_fields_only_applies_deleted_filter() {
        let filter = ContractFilter {
            contract_no: None,
            customer_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        assert_eq!(filter.to_doc(), doc! { "deleted_at": 0_i64 });
    }

    #[test]
    fn sort_doc_defaults_to_created_at_descending() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("contract_no"), true), doc! { "contract_no": 1 });
    }
}
