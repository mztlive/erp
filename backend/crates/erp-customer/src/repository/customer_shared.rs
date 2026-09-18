//! 客户仓储共享查询构造（erp-customer-001 拆分）。
//!
//! 半开有效期窗口、排序白名单、主责聚合管道、投影文档与列表投影行集中在此一处；
//! `customer.rs` 只保留筛选结构与仓储方法，查询语义保持不变。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::BusinessDate;
use mongodb::bson::{Document, doc};
use serde::{Deserialize, Serialize};

use crate::entity::customer::{AssignmentRole, CustomerAccountStatus};
use crate::repository::CustomerExt;

/// 对客户 ID 迭代器排序去重（授权并集、组织展开与投影归一化的唯一入口）。
///
/// # 参数
/// * `ids` - 原始客户 ID 迭代器（可含重复、无序）
///
/// # 返回
/// 返回升序去重后的客户 ID；空输入返回空集合。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存函数；固定升序保证投影顺序稳定，与业务日期边界无关。
pub(crate) fn distinct_sorted_customer_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut sorted: Vec<String> = ids.into_iter().collect();
    sorted.sort();
    sorted.dedup();
    sorted
}

/// 构造用户对目标客户的当前有效归属查询条件。
///
/// 有效期采用半开区间 `[valid_from, valid_to)`；`valid_to = null` 表示长期有效。
pub(crate) fn active_customer_user_assignment_filter(
    customer_id: &str,
    user_id: &str,
    as_of: BusinessDate,
) -> Document {
    active_window_filter(
        &as_of,
        Some(doc! {
            "customer_id": customer_id,
            "user_id": user_id,
            "assignment_role": {
                "$in": [
                    AssignmentRole::Owner.as_str(),
                    AssignmentRole::Collaborator.as_str(),
                ],
            },
        }),
    )
}

/// 组装半开有效期过滤条件的唯一入口（erp-customer-001）。
///
/// `list_active_for_customers`、`find_current_owner`、
/// `find_active_assignments_for_user`、`active_customer_user_assignment_filter`
/// 与 `current_owner_pipeline` 的 `valid_from $lte` 加 `$or(valid_to null/$gt)`
/// 语义相同；各查询只传自有约束，窗口部分只在此一处实现。
///
/// # 参数
/// * `as_of` - 业务日期边界
/// * `owned` - 自有约束（客户、用户、角色等）；`None` 表示仅窗口
///
/// # 返回
/// 返回含半开窗口的 MongoDB 过滤文档。
pub(crate) fn active_window_filter(as_of: &BusinessDate, owned: Option<Document>) -> Document {
    let as_of = as_of.to_string();
    let mut filter = owned.unwrap_or_default();
    filter.insert("valid_from", doc! { "$lte": &as_of });
    filter.insert("$or", vec![doc! { "valid_to": null }, doc! { "valid_to": { "$gt": &as_of } }]);
    filter
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
pub(crate) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|candidate| allowed.contains(candidate)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}

/// 构造当前主责查询：未删除归属和未删除客户共同受有效期及可见边界限制。
pub(crate) fn current_owner_pipeline(
    customer_ids: Option<&[String]>,
    owner_ids: Option<&[String]>,
    as_of: BusinessDate,
) -> Vec<Document> {
    let mut filter = active_window_filter(
        &as_of,
        Some(doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "assignment_role": "OWNER" }),
    );
    if let Some(ids) = customer_ids {
        filter.insert("customer_id", doc! { "$in": ids });
    }
    if let Some(ids) = owner_ids {
        filter.insert("user_id", doc! { "$in": ids });
    }
    vec![
        doc! { "$match": filter },
        doc! { "$lookup": { "from": <mongodb::Database as CustomerExt>::CUSTOMER_ACCOUNTS, "localField": "customer_id", "foreignField": "id", "as": "visible_customer" } },
        doc! { "$match": { "visible_customer": { "$elemMatch": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON } } } },
        doc! { "$unset": "visible_customer" },
    ]
}

/// 客户角色列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
pub(crate) fn customer_account_projection() -> Document {
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
pub(crate) fn customer_assignment_projection() -> Document {
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

impl CustomerAccountRow {
    /// 以稳定身份构造列表投影行，版本与时间戳从零开始。
    ///
    /// # 参数
    /// * `id` - 实体主键
    /// * `party_id` - 共用企业主体 ID
    /// * `customer_no` - 客户编号
    ///
    /// # 返回
    /// 返回启用状态的投影行。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: impl Into<String>, party_id: impl Into<String>, customer_no: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            party_id: party_id.into(),
            customer_no: customer_no.into(),
            default_payment_term_id: None,
            status: CustomerAccountStatus::Active,
            version: 0,
            created_at: 0,
            updated_at: 0,
        }
    }
}

/// 客户编号窄投影行。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CustomerNumberRow {
    /// 客户稳定 ID。
    pub(crate) id: String,
    /// 客户编号。
    pub(crate) customer_no: String,
}
