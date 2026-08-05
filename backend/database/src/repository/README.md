# Repository 模块

本模块集中封装 MongoDB 数据访问细节。Service 通过 `DatabaseExt` 获取实体仓储，
不直接拼装 MongoDB 查询或持有集合。

## 组成

- `base.rs`：通用 `Repository<'a, T>`、分页查询契约和乐观锁写入。
- `extensions.rs`：按实体暴露仓储访问器，并固定集合名称。
- `*_profile.rs`、`account_core.rs` 等：实体专用查询。
- `regex_filter.rs`：按字面量构造忽略大小写的正则查询，避免调用方重复转义。

## 通用能力

`Repository<'a, T>` 的每个方法都以 `executor: &mut dyn Executor` 结尾，提供：

- `create`
- `find_by_id`、`find_one`、`find_one_by_field`、`find_many`、`find_many_sorted`
- `update`
- `soft_delete`
- `restore`
- `list_all`、`exists`
- `search`：结合 `QueryFilter` 与 `Pagination` 返回 `PageResult<T>`

实体更新、软删除和恢复都使用 `id + version + deleted_at` 做状态与版本比较；
写入成功后同步内存实体的 `version`、`updated_at` 和 `deleted_at`。

## 分页查询

```rust
use database::repository::{PageResult, Pagination, QueryFilter, Repository};
use mongodb::bson::{doc, Document};

struct AccountFilter {
    page: u64,
    page_size: u32,
}

impl QueryFilter for AccountFilter {
    fn to_doc(&self) -> Document {
        doc! { "deleted_at": 0_i64 }
    }
}

impl Pagination for AccountFilter {
    fn skip(&self) -> u64 {
        (self.page.max(1) - 1) * u64::from(self.page_size)
    }

    fn limit(&self) -> i64 {
        i64::from(self.page_size)
    }
}

async fn query<T>(
    repository: &Repository<'_, T>,
    filter: &AccountFilter,
) -> database::Result<PageResult<T>>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    repository.search(filter, &mut database::NoTransaction).await
}
```

分页结果固定按 `created_at` 倒序返回 `items` 和 `total`。Filter 必须显式包含
软删除条件；实体专用 Filter 已统一处理。

## 实体专用查询

只有通用仓储无法准确表达的查询才放入实体文件，例如：

- `AccountCore` 按账号、账号类型或筛选条件查询。
- Profile 按 `account_id` 查询或批量查询。
- Role 查询启用角色。

方法使用 `find_*` 表达单项查找、`list_*` 表达集合查询、`has_*`/`exists`
表达存在性判断，并同样以执行器参数结尾。

## 事务边界

事务由 Service 通过 `database::Transactional` 发起；Repository 不自行开启或提交事务，
只按传入的执行器决定是否加入事务：

- 单集合读写传 `&mut NoTransaction`。
- 事务内调用把闭包中的 `&mut ClientSession` 直接作为执行器传入。

会话与非会话两套驱动调用形态收敛在 `database::mongo_ops`，Repository 因此只写一份实现。
先删后写等多步骤方法必须收到事务执行器，方法注释中已注明该约束。

## 扩展步骤

1. 在 `extensions.rs` 增加实体访问器并固定集合名称。
2. 通用 CRUD 直接复用 `Repository`。
3. 确需专用查询时新增对应模块，并隐藏 MongoDB 查询文档。
4. 为新增查询评估索引，在 `database/src/indexes.rs` 中维护索引定义。
5. 为过滤条件和关键边界补充测试。

不要为未来可能使用的查询预先增加 public helper；先由实际 Service 用例驱动接口。
