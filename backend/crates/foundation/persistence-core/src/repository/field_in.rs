//! 在领域提供的类型化集合上按字段批量读取未删除记录。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Collection;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use serde::de::DeserializeOwned;

use crate::{Executor, Result, mongo_ops};

/// 按领域指定的主键或关联字段批量取活跃记录。`$in` 使用原值、顺序和重复项，结果不承诺排序，也不分页或投影。
/// 空输入直接返回，不访问数据库或执行器。
pub async fn find_active_by_field_in<T>(
    collection: &Collection<T>,
    field: &str,
    values: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: DeserializeOwned + Send + Sync,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }
    mongo_ops::find_many(collection, active_field_filter(field, values), FindOptions::default(), executor)
        .await
}

fn active_field_filter(field: &str, values: &[String]) -> Document {
    doc! {
        field: { "$in": values },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::options::{ClientOptions, ServerAddress};
    use mongodb::{Client, ClientSession};

    use super::*;
    use crate::executor::sealed::Sealed;

    /// 空输入若触发会话访问则使测试失败。
    struct UnusedExecutor;

    impl Sealed for UnusedExecutor {}

    impl Executor for UnusedExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            panic!("empty input must not access a session")
        }
    }

    // 仅创建集合句柄并传入不可借会话的执行器，确认空匹配值在访问数据库前即返回空列表。
    #[tokio::test]
    async fn empty_values_return_without_accessing_executor() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let client = Client::with_options(options).unwrap();
        let collection = client.database("unit_test").collection::<Document>("documents");

        let rows = find_active_by_field_in(&collection, "parent_id", &[], &mut UnusedExecutor).await.unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn filter_keeps_exact_values_and_excludes_deleted_records() {
        let values = vec!["second".to_string(), "first".to_string(), "second".to_string()];

        assert_eq!(
            active_field_filter("parent_id", &values),
            doc! {
                "parent_id": { "$in": ["second", "first", "second"] },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
        );
        assert_eq!(
            active_field_filter("id", &[]),
            doc! { "id": { "$in": [] }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
        );
    }
}
