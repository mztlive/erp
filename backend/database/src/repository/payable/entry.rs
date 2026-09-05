use std::collections::HashMap;

use entities::common::time::BusinessDate;
use entities::ids::{PayableAccountId, PayableEntryId};
use entities::payable::{PayableEntry, PayableEntryOffset};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::Deserialize;

use super::super::Repository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 应付账户最早到期日聚合行。
#[derive(Debug, Deserialize)]
struct AccountDueDateRow {
    /// 应付账户 ID。
    #[serde(rename = "_id")]
    account_id: String,
    /// 最早到期日。
    due_date: BusinessDate,
}

/// 增加分录到期日最小投影行（FIN-R07 只投影到期日）。
#[derive(Debug, Deserialize)]
struct IncreaseDueDateRow {
    /// 到期日。
    due_date: BusinessDate,
}

impl<'a> Repository<'a, PayableEntry> {
    /// 按应付账户聚合最早分录到期日。
    ///
    /// # 参数
    /// * `account_ids` - 应付账户 ID；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 每个存在分录的账户至多返回一个最早到期日；无分录账户不上表。
    ///
    /// # 错误
    /// MongoDB 聚合或反序列化失败时返回错误。
    pub async fn minimum_due_dates_by_accounts(
        &self,
        account_ids: &[PayableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, BusinessDate>> {
        if account_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = account_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let pipeline = minimum_due_dates_pipeline("payable_account_id", ids, None);
        let rows = match executor.session() {
            Some(session) => {
                self.collection()
                    .aggregate(pipeline)
                    .with_type::<AccountDueDateRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                self.collection()
                    .aggregate(pipeline)
                    .with_type::<AccountDueDateRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|row| (row.account_id, row.due_date))
            .collect())
    }

    /// 批量按子账集合取回分录（`$in` 一次取回，禁止 N+1）。
    ///
    /// 用于账龄汇总与付款核销锁定；只返回未删除分录（事实类恒未删除）。
    ///
    /// # 参数
    /// * `account_ids` - 应付往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_accounts(
        &self,
        account_ids: &[PayableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntry>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "payable_account_id": { "$in": account_ids } }, executor)
            .await
    }

    /// 按主键集合批量取回应付分录（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `entry_ids` - 应付分录 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分录；空集合直接返回空列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_ids(
        &self,
        entry_ids: &[PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntry>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 按子账取回全部分录（按来源序号升序）。
    ///
    /// # 参数
    /// * `account_id` - 应付往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `source_sequence` 升序的全部分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_account(
        &self,
        account_id: &PayableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntry>> {
        self.find_many_sorted(
            doc! { "payable_account_id": account_id.to_string() },
            doc! { "source_sequence": 1 },
            executor,
        )
        .await
    }

    /// 按业务范围取最早增加分录到期日（FIN-R07）。
    ///
    /// 按子账过滤 `direction = increase`，只投影 `due_date`，按
    /// `(due_date, id)` 稳定升序取第一条；无增加分录时返回 `None`，
    /// 由 Service 转译既有“缺少增加分录”错误。空集不访问数据库；
    /// 查询形状由 `idx_payable_entries_account_direction_due` 覆盖。
    ///
    /// # 参数
    /// * `account_id` - 应付往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回确定性最早到期日；无增加分录时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn earliest_increase_due_date(
        &self,
        account_id: &PayableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<BusinessDate>> {
        use entities::payable::EntryDirection;
        let options = FindOptions::builder()
            .sort(doc! { "due_date": 1, "id": 1 })
            .limit(1)
            .projection(doc! { "due_date": 1 })
            .build();
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<IncreaseDueDateRow>(),
            doc! {
                "payable_account_id": account_id.to_string(),
                "direction": EntryDirection::Increase.as_str(),
            },
            options,
            executor,
        )
        .await?;
        Ok(rows.into_iter().next().map(|row| row.due_date))
    }
}

/// 构造按账户求最早到期日的聚合管道。
fn minimum_due_dates_pipeline(
    account_field: &str,
    account_ids: Vec<String>,
    direction: Option<&str>,
) -> Vec<Document> {
    let mut matched = doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        account_field: { "$in": account_ids },
    };
    if let Some(direction) = direction {
        matched.insert("direction", direction);
    }
    vec![
        doc! { "$match": matched },
        doc! {
            "$group": {
                "_id": format!("${account_field}"),
                "due_date": { "$min": "$due_date" },
            }
        },
        doc! { "$sort": { "_id": 1 } },
    ]
}

impl<'a> Repository<'a, PayableEntryOffset> {
    /// 按减少分录取回全部抵销（按抵销序号升序）。
    ///
    /// 用于校验「减少分录分配合计等于其金额」（数据模型 §6.9）。
    ///
    /// # 参数
    /// * `decrease_entry_id` - 减少分录 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `offset_sequence` 升序的抵销记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_offsets_by_decrease(
        &self,
        decrease_entry_id: &PayableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntryOffset>> {
        self.find_many_sorted(
            doc! { "decrease_entry_id": decrease_entry_id.to_string() },
            doc! { "offset_sequence": 1 },
            executor,
        )
        .await
    }

    /// 按增加分录取回被冲减的抵销集合。
    ///
    /// 用于校验「累计冲减不得超额」（数据模型 §6.9）。
    ///
    /// # 参数
    /// * `increase_entry_id` - 增加分录 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部引用该增加分录的抵销记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_offsets_by_increase(
        &self,
        increase_entry_id: &PayableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntryOffset>> {
        self.find_many(
            doc! { "increase_entry_id": increase_entry_id.to_string() },
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::minimum_due_dates_pipeline;
    use mongodb::bson::doc;

    #[test]
    fn minimum_due_date_pipeline_groups_one_row_per_account() {
        let pipeline = minimum_due_dates_pipeline(
            "payable_account_id",
            vec!["pa-1".to_string(), "pa-2".to_string()],
            None,
        );
        let matched = pipeline[0].get_document("$match").unwrap();
        assert_eq!(matched.get_i64("deleted_at").unwrap(), 0);
        assert!(matched.get_document("payable_account_id").is_ok());
        let group = pipeline[1].get_document("$group").unwrap();
        assert_eq!(group.get_str("_id").unwrap(), "$payable_account_id");
        assert_eq!(
            group.get_document("due_date").unwrap(),
            &doc! { "$min": "$due_date" }
        );
    }
}
