//! 结算主体公共目录；后台身份由入口认证，目录不消费 RBAC 或人员数据范围。
use application_core::directory::DirectoryQuery;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;
use crate::dto::directory::SettlementPartyDirectoryPage;
use crate::repository::directory::snapshot;

/// 后台登录后的我方公司轻量目录。
pub struct SettlementPartyDirectoryService {
    db: Database,
}
impl SettlementPartyDirectoryService {
    /// 装配本域目录读取。
    /// # 参数
    /// `db` 为本域数据库。
    /// # 返回
    /// 不执行 I/O 的服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 查询目录或已选身份，不读取角色及人员范围。
    /// # 参数
    /// `query` 为搜索、分页或已选身份条件。
    /// # 返回
    /// 我方公司目录页，只含编号、名称和状态。
    /// # 错误
    /// 非法请求、目录超限、内容版本变化或持久化失败时拒绝。
    pub async fn list(&self, query: DirectoryQuery) -> Result<SettlementPartyDirectoryPage> {
        let query = query.normalized()?;
        let rows = snapshot(&self.db, &query, &mut NoTransaction).await?;
        SettlementPartyDirectoryPage::from_snapshot(rows, &query)
    }
}
