//! 域 D08 `customer` 仓储：customer_account、customer_assignment（数据模型 §6.2）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充
//! 域特有查询与跨集合多步骤写入入口。`customer_account` 是稳定基础资料
//! （可软删除，身份类字段全局唯一），`customer_assignment` 是按有效期保存的
//! 归属事实行（追加维护，**不提供**软删除）。
//!
//! 集合名常量统一从 `CustomerExt` 关联常量导入（唯一权威来源）；筛选/行类型
//! 定义在本文件，经 `CustomerExt` 的关联类型对外暴露。

use entities::common::time::BusinessDate;
use entities::customer::{
    AssignmentRole, CustomerAccount, CustomerAccountStatus, CustomerAssignment, CustomerProfileCommand,
};
use entities::ids::{CustomerAccountId, PartyId};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 客户角色列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAccountRow {
    /// 实体主键。
    pub id: String,
    /// 共用企业主体 ID。
    pub party_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// 默认客户付款条件引用。
    pub default_payment_term_id: Option<String>,
    /// 启停状态。
    pub status: CustomerAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 最后更新时间（秒级时间戳）。
    pub updated_at: u64,
}

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

impl<'a> Repository<'a, CustomerAccount> {
    /// 按客户角色 ID 集合批量读取活跃客户。
    pub async fn find_accounts_by_ids(
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
    pub async fn find_customer(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAccount>> {
        self.find_by_id(id, executor).await
    }

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
    pub async fn search_customer_accounts(
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

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

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
    pub async fn find_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAccount>> {
        self.find_one(doc! { "party_id": party_id.to_string() }, executor)
            .await
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

impl<'a> Repository<'a, CustomerAssignment> {
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
    pub async fn find_assignment(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAssignment>> {
        self.find_by_id(id, executor).await
    }

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
    pub async fn list_for_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        self.find_many(doc! { "customer_id": customer_id.to_string() }, executor)
            .await
    }

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
    pub async fn list_history_for_customer(
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
    pub async fn list_active_for_customers(
        &self,
        customer_ids: &[String],
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        if customer_ids.is_empty() {
            return Ok(Vec::new());
        }
        let as_of = as_of.to_string();
        self.find_many(
            doc! {
                "customer_id": { "$in": customer_ids },
                "valid_from": { "$lte": &as_of },
                "$or": [
                    { "valid_to": null },
                    { "valid_to": { "$gt": &as_of } },
                ],
            },
            executor,
        )
        .await
    }

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
    pub async fn find_current_owner(
        &self,
        customer_id: &CustomerAccountId,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAssignment>> {
        let as_of = as_of.to_string();
        Ok(self
            .find_many_sorted(
                doc! {
                    "customer_id": customer_id.to_string(),
                    "assignment_role": AssignmentRole::Owner.as_str(),
                    "valid_from": { "$lte": &as_of },
                    "$or": [
                        { "valid_to": null },
                        { "valid_to": { "$gt": &as_of } },
                    ],
                },
                doc! { "valid_from": -1, "created_at": -1 },
                executor,
            )
            .await?
            .into_iter()
            .next())
    }

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
    pub async fn search_customer_assignments(
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

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

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
    pub async fn find_active_assignments_for_user(
        &self,
        user_id: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        let as_of = as_of.to_string();
        self.find_many(
            doc! {
                "user_id": user_id,
                "valid_from": { "$lte": &as_of },
                "$or": [
                    { "valid_to": null },
                    { "valid_to": { "$gt": &as_of } },
                ],
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, CustomerProfileCommand> {
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
    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerProfileCommand>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor)
            .await
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

/// 客户角色列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_account_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "customer_no": 1,
        "default_payment_term_id": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
        "updated_at": 1,
    }
}

/// 客户归属列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_assignment_projection() -> Document {
    doc! {
        "id": 1,
        "customer_id": 1,
        "user_id": 1,
        "assignment_role": 1,
        "valid_from": 1,
        "valid_to": 1,
        "change_reason": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, CustomerAccountFilter, QueryFilter};
    use entities::customer::CustomerAccountStatus;
    use mongodb::bson::doc;

    #[test]
    fn customer_account_filter_applies_keyword_and_status() {
        let filter = CustomerAccountFilter {
            keyword: Some("C-".to_string()),
            keyword_party_ids: None,
            party_id: None,
            party_ids: None,
            customer_ids: None,
            status: Some(CustomerAccountStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
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
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("customer_no"), true, &["created_at", "customer_no"]),
            doc! { "customer_no": 1 }
        );
    }
}
