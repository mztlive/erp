use std::sync::Arc;

use mongodb::Database;

use crate::projection::connector::MallConnector;

/// 执行投影服务。
pub struct ProjectionService {
    pub(super) db: Database,
    pub(super) connector: Arc<dyn MallConnector>,
}

impl ProjectionService {
    /// 创建执行投影服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `connector` - 商城连接器
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, connector: Arc<dyn MallConnector>) -> Self {
        Self { db, connector }
    }
}
