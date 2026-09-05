use entities::common::time::BusinessDate;
use entities::ids::{ReceivableAccountId, ReceivableEntryId};
use entities::receivable::{ReceivableEntry, ReceivableEntryOffset, ReceivableFundsReview};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use serde::Deserialize;

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

/// 应收账户最早到期日聚合行。
#[derive(Debug, Deserialize)]
struct AccountDueDateRow {
    /// 应收账户 ID。
    #[serde(rename = "_id")]
    account_id: String,
    /// 最早到期日。
    due_date: BusinessDate,
}

impl<'a> Repository<'a, ReceivableEntry> {
    /// 按应收账户聚合最早正向分录到期日。
    ///
    /// # 参数
    /// * `account_ids` - 应收账户 ID；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 每个存在 Increase 分录的账户至多返回一个最早到期日；Decrease 与无分录账户不上表。
    ///
    /// # 错误
    /// MongoDB 聚合或反序列化失败时返回错误。
    pub async fn minimum_increase_due_dates_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<std::collections::HashMap<String, BusinessDate>> {
        if account_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let ids = account_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let pipeline = minimum_due_dates_pipeline(ids);
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
    /// 用于账龄汇总与开票核销锁定；只返回未删除分录（事实类恒未删除）。
    ///
    /// # 参数
    /// * `account_ids` - 应收往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntry>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "receivable_account_id": { "$in": account_ids } }, executor)
            .await
    }

    /// 按子账取回全部分录（按来源序号升序）。
    ///
    /// # 参数
    /// * `account_id` - 应收往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `source_sequence` 升序的全部分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_account(
        &self,
        account_id: &ReceivableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntry>> {
        self.find_many_sorted(
            doc! { "receivable_account_id": account_id.to_string() },
            doc! { "source_sequence": 1 },
            executor,
        )
        .await
    }
}

/// 构造应收正向分录最早到期日聚合管道。
fn minimum_due_dates_pipeline(account_ids: Vec<String>) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "receivable_account_id": { "$in": account_ids },
                "direction": entities::receivable::EntryDirection::Increase.as_str(),
            }
        },
        doc! {
            "$group": {
                "_id": "$receivable_account_id",
                "due_date": { "$min": "$due_date" },
            }
        },
        doc! { "$sort": { "_id": 1 } },
    ]
}

impl<'a> Repository<'a, ReceivableEntryOffset> {
    /// 按减少分录集合批量取回抵销记录。
    ///
    /// # 参数
    /// * `decrease_entry_ids` - 减少分录 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配抵销记录；调用方按分录分组后按抵销序号排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_offsets_by_decreases(
        &self,
        decrease_entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryOffset>> {
        if decrease_entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = decrease_entry_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        self.find_many(doc! { "decrease_entry_id": { "$in": ids } }, executor)
            .await
    }

    /// 按减少分录取回全部抵销（按抵销序号升序）。
    ///
    /// 用于校验「减少分录分配合计等于其金额」（数据模型 §6.8）。
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
        decrease_entry_id: &ReceivableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryOffset>> {
        self.find_many_sorted(
            doc! { "decrease_entry_id": decrease_entry_id.to_string() },
            doc! { "offset_sequence": 1 },
            executor,
        )
        .await
    }

    /// 按增加分录取回被冲减的抵销集合。
    ///
    /// 用于校验「每笔增加分录累计净冲减不超过原增加金额」（数据模型 §6.8）。
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
        increase_entry_id: &ReceivableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryOffset>> {
        self.find_many(
            doc! { "increase_entry_id": increase_entry_id.to_string() },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ReceivableFundsReview> {
    /// 按应收子账集合批量取回复核记录。
    ///
    /// # 参数
    /// * `account_ids` - 应收子账 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配复核记录；调用方按子账分组后按复核号排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_reviews_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableFundsReview>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = account_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "receivable_account_id": { "$in": ids } }, executor)
            .await
    }

    /// 按子账取回复核链全部记录（按复核号升序）。
    ///
    /// # 参数
    /// * `account_id` - 应收往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `review_no` 升序的复核记录；空链返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_reviews_by_account(
        &self,
        account_id: &ReceivableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableFundsReview>> {
        self.find_many_sorted(
            doc! { "receivable_account_id": account_id.to_string() },
            doc! { "review_no": 1 },
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
    fn minimum_due_date_pipeline_excludes_decrease_entries() {
        let pipeline = minimum_due_dates_pipeline(vec!["ra-1".to_string()]);
        let matched = pipeline[0].get_document("$match").unwrap();
        assert_eq!(matched.get_str("direction").unwrap(), "increase");
        let group = pipeline[1].get_document("$group").unwrap();
        assert_eq!(group.get_str("_id").unwrap(), "$receivable_account_id");
        assert_eq!(
            group.get_document("due_date").unwrap(),
            &doc! { "$min": "$due_date" }
        );
    }
}
