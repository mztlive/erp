//! 域 D08 `customer` 仓储：customer_account、customer_assignment（数据模型 §6.2）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充
//! 域特有查询与跨集合多步骤写入入口。`customer_account` 是稳定基础资料
//! （可软删除，身份类字段全局唯一），`customer_assignment` 是按有效期保存的
//! 归属事实行（追加维护，**不提供**软删除）。
//!
//! 集合名常量统一从 `CustomerExt` 关联常量导入（唯一权威来源）；筛选条件
//! 定义在本文件，经 `CustomerExt` 的关联类型对外暴露。

#![allow(async_fn_in_trait)]

use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, PartyId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

pub use super::customer_shared::CustomerAccountRow;
use super::customer_shared::{
    CustomerNumberRow, active_customer_user_assignment_filter, active_window_filter, current_owner_pipeline,
    customer_account_projection, customer_assignment_projection, distinct_sorted_customer_ids, sort_doc,
};
use crate::entity::customer::{
    AssignmentRole, CustomerAccount, CustomerAccountStatus, CustomerAssignment, CustomerProfileCommand,
};

/// 客户角色列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerAccountFilter {
    /// 客户编号模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 客户关键词命中的主体 ID，用于与客户编号组成同一 OR 条件。
    pub keyword_party_ids: Option<Vec<String>>,
    /// 共用企业主体 ID（精确匹配）；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 允许的 Party ID 集合；`Some(empty)` 表示范围内无客户。
    pub party_ids: Option<Vec<String>>,
    /// 允许的客户 ID 集合；`Some(empty)` 表示范围内无客户。
    pub customer_ids: Option<Vec<String>>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<CustomerAccountStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for CustomerAccountFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            keyword: None,
            keyword_party_ids: None,
            party_id: None,
            party_ids: None,
            customer_ids: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for CustomerAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(keyword) = self.keyword.as_deref() {
            let mut customer_no = Document::new();
            insert_literal_regex_filter(&mut customer_no, "customer_no", Some(keyword));
            let mut alternatives = vec![customer_no];
            if let Some(party_ids) = &self.keyword_party_ids {
                alternatives.push(doc! { "party_id": { "$in": party_ids } });
            }
            filter.insert("$or", alternatives);
        }
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        if let Some(party_ids) = &self.party_ids {
            filter.insert("party_id", doc! { "$in": party_ids });
        }
        if let Some(customer_ids) = &self.customer_ids {
            filter.insert("id", doc! { "$in": customer_ids });
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CustomerAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 客户账户集合上的域查询。
#[allow(async_fn_in_trait)]
pub trait CustomerAccountRepositoryExt {
    /// 批量读取未删除客户的稳定 ID 与客户编号。
    ///
    /// # 参数
    /// * `customer_ids` - 客户 ID 集合；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回实际存在的客户编号映射；停用但未删除客户仍保留编号，软删除或缺失不补行。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn customer_numbers_by_ids(
        &self,
        customer_ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;

    /// 按客户角色 ID 集合批量读取活跃客户。
    async fn find_accounts_by_ids(
        &self,
        customer_ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAccount>>;

    /// 按客户角色 ID 查找未删除客户。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除客户；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_customer(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<CustomerAccount>>;

    /// 分页检索客户角色列表（投影查询）。
    ///
    /// 只返回 [`CustomerAccountRow`] 所需的列表字段，不加载整文档；排序字段
    /// 经仓储白名单校验（`created_at`/`updated_at`/`customer_no`/`status`），非法字段回落
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
    async fn search_customer_accounts(
        &self,
        filter: &CustomerAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerAccountRow>>;

    /// 按共用企业主体查找客户角色（一个主体至多一个客户角色，由
    /// `uk_customer_accounts_party` 保证）。
    ///
    /// # 参数
    /// * `party_id` - 共用企业主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除客户角色；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAccount>>;

    /// 返回匹配条件的未删除对象 ID，供跨域列表在分页前组合筛选。
    ///
    /// 数据库查询失败时返回错误；空命中返回空集合，不扩大范围。
    async fn matching_ids_by_parties(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

impl CustomerAccountRepositoryExt for Repository<'_, CustomerAccount> {
    async fn customer_numbers_by_ids(
        &self,
        customer_ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if customer_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = customer_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let collection = self.collection().clone_with_type::<CustomerNumberRow>();
        let rows = mongo_ops::find_many(
            &collection,
            doc! {
                "id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().projection(doc! { "id": 1, "customer_no": 1 }).build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| (row.id, row.customer_no)).collect())
    }

    async fn find_accounts_by_ids(
        &self,
        customer_ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAccount>> {
        if customer_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = customer_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    async fn find_customer(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<CustomerAccount>> {
        self.find_by_id(id, executor).await
    }

    async fn search_customer_accounts(
        &self,
        filter: &CustomerAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "updated_at", "customer_no", "status"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn find_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAccount>> {
        self.find_one(doc! { "party_id": party_id.to_string() }, executor).await
    }

    async fn matching_ids_by_parties(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        filter.insert("party_id", doc! { "$in": party_ids });
        let collection = self.collection();
        let mut query = collection.distinct("id", filter);
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        Ok(query.await?.into_iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
    }
}

/// 客户归属列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAssignmentRow {
    /// 实体主键。
    pub id: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 销售人员。
    pub user_id: String,
    /// 归属角色。
    pub assignment_role: AssignmentRole,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 调整原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户归属列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerAssignmentFilter {
    /// 客户角色 ID；`None` 表示不筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 销售人员；`None` 表示不筛选。
    pub user_id: Option<String>,
    /// 归属角色；`None` 表示不筛选。
    pub assignment_role: Option<AssignmentRole>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CustomerAssignmentFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id.to_string());
        }
        if let Some(user_id) = &self.user_id {
            filter.insert("user_id", user_id);
        }
        if let Some(role) = self.assignment_role {
            filter.insert("assignment_role", role.as_str());
        }
        filter
    }
}

impl Pagination for CustomerAssignmentFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 客户归属集合上的域查询。
#[allow(async_fn_in_trait)]
pub trait CustomerAssignmentRepositoryExt {
    /// 在客户域内批量读取当前主责；可见客户边界与人员筛选共同求交。
    ///
    /// # 参数
    /// * `customer_ids` - 可见客户边界；空集合保持为空，`None` 使用既有全量权限
    /// * `owner_ids` - 额外收窄的负责人集合
    /// * `as_of` - 归属有效期判定业务日期
    /// * `executor` - 事务或无事务执行器
    ///
    /// # 返回值
    /// 返回未删除客户的生效主责关系。
    ///
    /// # 错误
    /// 查询或归属关系反序列化失败向上传播。
    async fn current_owners(
        &self,
        customer_ids: Option<&[String]>,
        owner_ids: Option<&[String]>,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>>;

    /// 判断用户是否在指定日期拥有目标客户的有效归属。
    ///
    /// 查询同时约束客户、用户、OWNER/COLLABORATOR 角色与半开有效期；
    /// 通用 Repository 自动追加未删除条件，并以存在性投影停止在首条命中。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `user_id` - 销售人员 ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 命中当前有效 OWNER 或 COLLABORATOR 归属时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn has_active_assignment_for_customer_user(
        &self,
        customer_id: &str,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 按归属 ID 查找未删除客户归属。
    ///
    /// # 参数
    /// * `id` - 客户归属 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除归属；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_assignment(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAssignment>>;

    /// 读取指定客户的全部归属行。
    ///
    /// 该查询供事务内执行归属换任冲突计算；领域冲突规则由
    /// [`CustomerAssignment`] 自身判断。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该客户全部未删除归属。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_for_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>>;

    /// 按生效开始日与创建时间倒序读取客户归属历史。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最近生效的归属优先的完整历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_history_for_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>>;

    /// 批量读取指定客户在业务日期生效的全部归属。
    ///
    /// # 参数
    /// * `customer_ids` - 客户角色 ID 集合；为空时直接返回空集合
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回指定日期生效的 OWNER 与 COLLABORATOR 归属。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_active_for_customers(
        &self,
        customer_ids: &[String],
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>>;

    /// 查找客户在指定日期生效的负责人归属。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前 OWNER；没有生效负责人时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_current_owner(
        &self,
        customer_id: &CustomerAccountId,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAssignment>>;

    /// 分页检索客户归属列表（投影查询）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`valid_from`/`valid_to`），
    /// 非法字段回落默认 `created_at`。
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
    async fn search_customer_assignments(
        &self,
        filter: &CustomerAssignmentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerAssignmentRow>>;

    /// 检索某销售人员在指定业务日期生效的归属（「我的客户」查询，§6.2）。
    ///
    /// 归属有效期按 ISO 日期字符串比较；`valid_to` 为 `None` 的开放区间
    /// 视为长期有效。该查询由 `idx_customer_assignments_user` 支撑。
    ///
    /// # 参数
    /// * `user_id` - 销售人员
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该业务日期生效的归属行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_active_assignments_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>>;

    /// 去重后的生效客户 ID 投影（INT-R22）。
    ///
    /// 一次归属查询装载调用方当天的生效行，在仓储内按客户 ID 排序去重后返回；
    /// 空结果返回空集合。业务日期边界与半开有效期语义与
    /// [`Self::find_active_assignments_for_user`] 完全一致。
    ///
    /// # 参数
    /// * `user_id` - 销售人员
    /// * `as_of` - 业务日期边界，由 Service 显式注入
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回排序去重后的客户 ID；无生效归属时为空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回强类型客户 ID 投影，不返回 services DTO、HTTP View 或授权结论；
    /// 候选排序与数据范围仍由 Service 解释。
    async fn distinct_active_customer_ids_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

impl CustomerAssignmentRepositoryExt for Repository<'_, CustomerAssignment> {
    async fn current_owners(
        &self,
        customer_ids: Option<&[String]>,
        owner_ids: Option<&[String]>,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        let pipeline = current_owner_pipeline(customer_ids, owner_ids, as_of);
        let collection = self.collection();
        let mut result = Vec::new();
        if let Some(session) = executor.session() {
            let mut cursor = collection
                .aggregate(pipeline)
                .with_type::<CustomerAssignment>()
                .session(&mut *session)
                .await?;
            while cursor.advance(session).await? {
                result.push(cursor.deserialize_current()?);
            }
            return Ok(result);
        }
        let mut cursor = collection.aggregate(pipeline).with_type::<CustomerAssignment>().await?;
        while cursor.advance().await? {
            result.push(cursor.deserialize_current()?);
        }
        Ok(result)
    }

    async fn has_active_assignment_for_customer_user(
        &self,
        customer_id: &str,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.exists(active_customer_user_assignment_filter(customer_id, user_id, as_of), executor).await
    }

    async fn find_assignment(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAssignment>> {
        self.find_by_id(id, executor).await
    }

    async fn list_for_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        self.find_many(doc! { "customer_id": customer_id.to_string() }, executor).await
    }

    async fn list_history_for_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        self.find_many_sorted(
            doc! { "customer_id": customer_id.to_string() },
            doc! { "valid_from": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    async fn list_active_for_customers(
        &self,
        customer_ids: &[String],
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        if customer_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            active_window_filter(&as_of, Some(doc! { "customer_id": { "$in": customer_ids } }), None),
            executor,
        )
        .await
    }

    async fn find_current_owner(
        &self,
        customer_id: &CustomerAccountId,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAssignment>> {
        Ok(self
            .find_many_sorted(
                active_window_filter(
                    &as_of,
                    Some(doc! {
                        "customer_id": customer_id.to_string(),
                        "assignment_role": AssignmentRole::Owner.as_str(),
                    }),
                    None,
                ),
                doc! { "valid_from": -1, "created_at": -1 },
                executor,
            )
            .await?
            .into_iter()
            .next())
    }

    async fn search_customer_assignments(
        &self,
        filter: &CustomerAssignmentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerAssignmentRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "valid_from", "valid_to"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_assignment_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerAssignmentRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn find_active_assignments_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        self.find_many(active_window_filter(&as_of, Some(doc! { "user_id": user_id }), None), executor).await
    }

    async fn distinct_active_customer_ids_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let assignments = self.find_active_assignments_for_user(user_id, as_of, executor).await?;
        Ok(distinct_sorted_customer_ids(
            assignments.iter().map(|assignment| assignment.customer_id.to_string()),
        ))
    }
}

/// 客户资料命令集合上的域查询。
#[allow(async_fn_in_trait)]
pub trait CustomerProfileCommandRepositoryExt {
    /// 按客户端幂等键读取已成功命令结果。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端生成且重试时保持不变的幂等键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回已提交的稳定命令结果；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerProfileCommand>>;
}

impl CustomerProfileCommandRepositoryExt for Repository<'_, CustomerProfileCommand> {
    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerProfileCommand>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor).await
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::super::customer_shared::{
        active_customer_user_assignment_filter, current_owner_pipeline, distinct_sorted_customer_ids,
        sort_doc,
    };
    use super::CustomerAccountFilter;
    use crate::entity::customer::CustomerAccountStatus;

    #[test]
    fn current_owners_preserve_empty_scope_and_exclude_deleted_assignments() {
        let day = BusinessDate::from_ymd(2026, 9, 13).unwrap();
        let pipeline = current_owner_pipeline(Some(&[]), Some(&["user-1".into()]), day);
        let filter = pipeline[0].get_document("$match").unwrap();
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(filter.get_str("assignment_role").unwrap(), "OWNER");
        assert_eq!(filter.get_document("customer_id").unwrap(), &doc! { "$in": [] });
        assert_eq!(filter.get_document("user_id").unwrap(), &doc! { "$in": ["user-1"] });
        assert_eq!(filter.get_document("valid_from").unwrap(), &doc! { "$lte": "2026-09-13" });
        assert_eq!(
            filter.get_array("$or").unwrap()[1],
            mongodb::bson::Bson::Document(doc! { "valid_to": { "$gt": "2026-09-13" } })
        );
        assert_eq!(
            pipeline[2],
            doc! { "$match": { "visible_customer": { "$elemMatch": { "deleted_at": 0_i64 } } } }
        );
    }

    #[test]
    fn customer_account_row_new_carries_identity_fields() {
        let row = super::CustomerAccountRow::new("customer-1", "party-1", "C-1");
        assert_eq!(row.id, "customer-1");
        assert_eq!(row.party_id, "party-1");
        assert_eq!(row.customer_no, "C-1");
        assert_eq!(row.default_payment_term_id, None);
        assert_eq!(row.status, CustomerAccountStatus::Active);
        assert_eq!(row.version, 0);
    }

    #[test]
    fn customer_account_filter_applies_keyword_and_status() {
        let filter = CustomerAccountFilter {
            keyword: Some("C-".to_string()),
            status: Some(CustomerAccountStatus::Active),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("status").unwrap(), "active");
        let keyword = document
            .get_array("$or")
            .unwrap()
            .first()
            .unwrap()
            .as_document()
            .unwrap()
            .get_document("customer_no")
            .unwrap();
        assert_eq!(keyword.get_str("$regex").unwrap(), r"C\-");
    }

    #[test]
    fn sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted() {
        assert_eq!(
            sort_doc(Some("revised_at"), false, &["created_at", "customer_no"]),
            doc! { "created_at": -1, "id": -1 }
        );
        assert_eq!(
            sort_doc(Some("customer_no"), true, &["created_at", "customer_no"]),
            doc! { "customer_no": 1, "id": 1 }
        );
    }

    #[test]
    fn active_customer_user_assignment_filter_preserves_scope_and_half_open_window() {
        let as_of = BusinessDate::from_str("2026-08-31").unwrap();

        let filter = active_customer_user_assignment_filter("customer-1", "user-1", as_of);

        assert_eq!(filter.get_str("customer_id").unwrap(), "customer-1");
        assert_eq!(filter.get_str("user_id").unwrap(), "user-1");
        assert_eq!(
            filter.get_document("assignment_role").unwrap().get_array("$in").unwrap(),
            &vec!["OWNER".into(), "COLLABORATOR".into()]
        );
        assert_eq!(filter.get_document("valid_from").unwrap().get_str("$lte").unwrap(), "2026-08-31");
        let valid_to = filter.get_array("$or").unwrap();
        assert!(valid_to[0].as_document().unwrap().get("valid_to").unwrap().as_null().is_some());
        assert_eq!(
            valid_to[1].as_document().unwrap().get_document("valid_to").unwrap().get_str("$gt").unwrap(),
            "2026-08-31"
        );
    }

    #[test]
    fn distinct_sorted_customer_ids_dedups_and_sorts() {
        assert!(distinct_sorted_customer_ids(Vec::<String>::new()).is_empty());
        assert_eq!(
            distinct_sorted_customer_ids(vec![
                "c-2".to_string(),
                "c-1".to_string(),
                "c-2".to_string(),
                "c-10".to_string(),
            ]),
            vec!["c-1".to_string(), "c-10".to_string(), "c-2".to_string()]
        );
    }

    #[test]
    fn customer_account_and_assignment_bson_roundtrip() {
        use erp_core::ids::PartyId;

        use crate::entity::customer::{
            AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
            CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
        };

        let account = CustomerAccount::new(
            CustomerAccountId::new("customer-4"),
            CustomerAccountData {
                party_id: PartyId::new("party-1"),
                customer_no: "C-2026-001".to_string(),
                default_payment_term_id: Some("POSTPAY_NET30".to_string()),
                status: CustomerAccountStatus::Active,
            },
            "admin-1",
        )
        .unwrap();
        let roundtrip: CustomerAccount =
            mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&account).unwrap())
                .unwrap();
        assert_eq!(roundtrip, account);

        let assignment = CustomerAssignment::new(
            CustomerAssignmentId::new("assign-5"),
            CustomerAssignmentData {
                customer_id: CustomerAccountId::new("customer-1"),
                user_id: "sales-zhangsan".to_string(),
                assignment_role: AssignmentRole::Owner,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
                change_reason: "首次指派".to_string(),
            },
        )
        .unwrap();
        let roundtrip: CustomerAssignment = mongodb::bson::deserialize_from_document(
            mongodb::bson::serialize_to_document(&assignment).unwrap(),
        )
        .unwrap();
        assert_eq!(roundtrip, assignment);
    }

    #[test]
    fn customer_profile_command_persists_bare_fingerprint_without_request_body() {
        use crate::entity::customer::{
            CustomerProfileCommand, CustomerProfileCommandData, CustomerProfileOperation,
            CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
        };

        let digest = "0".repeat(64);
        let command = CustomerProfileCommand::new(
            "command-legacy",
            CustomerProfileCommandData {
                idempotency_key: "customer-save-1".to_string(),
                operation: "update".to_string(),
                initiated_by: "admin-1".to_string(),
                request_fingerprint: digest.clone(),
                customer_id: "customer-1".to_string(),
                customer_no: "KH-1".to_string(),
                party_id: "party-1".to_string(),
                revision_id: "revision-2".to_string(),
                revision_no: 2,
                customer_version: 2,
                party_version: 2,
                effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
                change_reason: "资料修订".to_string(),
            },
        )
        .unwrap();
        let document = mongodb::bson::serialize_to_document(&command).unwrap();
        assert_eq!(document.get_str("operation").unwrap(), "update");
        assert_eq!(document.get_str("request_fingerprint").unwrap(), digest);
        assert_eq!(document.get_str("customer_id").unwrap(), "customer-1");
        assert!(!document.contains_key("request"));
        assert!(!document.contains_key("mobile"));
        let roundtrip: CustomerProfileCommand =
            mongodb::bson::deserialize_from_document(document).expect("历史命令 BSON 必须可回读");
        assert_eq!(roundtrip, command);

        let fingerprint = CustomerProfileRequestFingerprint::parse_compatible(&digest).unwrap();
        let context = CustomerProfileReplayContext::new(
            "customer-save-1",
            CustomerProfileOperation::Update,
            Some("customer-1".to_string()),
            "admin-1",
            fingerprint,
        )
        .unwrap();
        assert!(command.ensure_replay_matches(&context).is_ok());
    }
}
