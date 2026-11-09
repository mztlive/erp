//! 责任队列列表与单条详情查询。

use super::facts::WorkbenchFactsReader;
/// Cross-domain workbench query service.
///
/// 查询入口使用固定 Future 类型，授权实现和内部查询状态机不进入调用方的类型。
#[derive(Clone)]
pub struct WorkbenchReadService {
    /// MongoDB handle used for work-item and brief reads.
    pub db: mongodb::Database,
}

impl WorkbenchReadService {
    /// 构造与授权类型无关的对象读取器，不执行查询或授权。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回借用当前数据库的对象事实读取器。
    /// # 错误
    /// 不返回错误。
    pub(super) fn object_reader(&self) -> WorkbenchFactsReader<'_> {
        WorkbenchFactsReader::new(&self.db)
    }

    /// 用当前数据库构造工作项事实读取器，不执行查询。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定 `self.db` 克隆的 `WorkItemFactsReader`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn facts_reader(&self) -> super::authority::WorkItemFactsReader {
        super::authority::WorkItemFactsReader::new(self.db.clone())
    }

    /// 构造工作台读模型。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回绑定当前授权源的工作台读模型。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
