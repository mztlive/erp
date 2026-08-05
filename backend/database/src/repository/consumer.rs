use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::errors::Result;
use entities::Consumer;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

impl<'a> Repository<'a, Consumer> {
    /// 根据账号查找消费者
    ///
    /// # 参数
    /// * `account` - 账号
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn find_by_account(&self, account: &str) -> Result<Option<Consumer>> {
        self.find_one_by_field("account", account).await
    }

    /// 根据账号查找消费者，包含已软删除记录。
    ///
    /// 全局唯一索引包含软删除记录；账号占用校验必须使用本方法。
    ///
    /// # 参数
    /// * `account` - 账号
    ///
    /// # 返回
    /// 返回匹配的消费者
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_account_including_deleted(&self, account: &str) -> Result<Option<Consumer>> {
        let consumer = self
            .database()
            .collection::<Consumer>(self.collection_name())
            .find_one(doc! { "account": account })
            .await?;
        Ok(consumer)
    }

    /// 按条件检索消费者列表
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn search_consumers(&self, filter: &ConsumerFilter) -> Result<PageResult<Consumer>> {
        self.search(filter).await
    }
}

/// 消费者列表过滤条件
#[derive(Debug, Clone)]
pub struct ConsumerFilter {
    pub account: Option<String>,
    pub nickname: Option<String>,
    pub page: u64,
    pub page_size: u32,
}

impl QueryFilter for ConsumerFilter {
    /// 转换为`doc`表示。
    ///
    /// # 返回
    /// 返回 `Document` 实例。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };

        insert_literal_regex_filter(&mut filter, "account", self.account.as_deref());
        insert_literal_regex_filter(&mut filter, "nickname", self.nickname.as_deref());

        filter
    }
}

impl Pagination for ConsumerFilter {
    /// 返回页码和分页大小。
    ///
    /// # 返回
    /// 返回原始页码与单页条目数。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

#[cfg(test)]
mod tests {
    use super::{ConsumerFilter, QueryFilter};

    #[test]
    fn account_filter_treats_regex_metacharacters_as_literal_text() {
        let filter = ConsumerFilter {
            account: Some("a.b".to_string()),
            nickname: None,
            page: 1,
            page_size: 20,
        }
        .to_doc();

        assert_eq!(
            filter.get_document("account").unwrap().get_str("$regex").unwrap(),
            r"a\.b"
        );
    }
}
