//! 销售与采购人员查询目录。页面只调用这里，不各自拼账号或业务记录。

mod grant;
pub mod management;
mod query;
mod sync;

use application_core::AuditActor;
pub use grant::grant_assigned_roles;
use mongodb::Database;
use persistence_core::Transactional;
pub use sync::sync_role_grants;

use crate::Result;
use crate::dto::{PersonDirectoryPage, PersonDirectoryQuery};
use crate::entity::person_directory::{DirectoryListRequest, PersonDirectoryCategory};
use crate::service::iam::SharedRbacService;

/// 人员目录用例。
pub struct PersonDirectoryService {
    db: Database,
    rbac: SharedRbacService,
}

impl PersonDirectoryService {
    /// 绑定身份库和既有 RBAC 服务。
    ///
    /// # 参数
    /// * `db` - 身份数据库
    /// * `rbac` - 共享 RBAC 服务
    ///
    /// # 返回
    /// 返回人员目录服务。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 分页查询某一固定类别的人员目录。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `category` - 路由固定的类别
    /// * `query` - 目录自身的搜索、分页和组织筛选
    ///
    /// # 返回
    /// 返回目录页。
    ///
    /// # 错误
    /// 参数、权限、版本或读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 授权、资格与组织在同一个事务读取；本方法不接收业务列表条件。
    pub async fn list(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        query: PersonDirectoryQuery,
    ) -> Result<PersonDirectoryPage> {
        let request = query.into_request()?;
        self.read(actor, category, Read::List(request)).await
    }

    /// 按已选 ID 回显当前仍可读的人员。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `category` - 路由固定的类别
    /// * `ids` - 已选账号 ID
    ///
    /// # 返回
    /// 返回可读人员；无权或不存在的 ID 被省略。
    ///
    /// # 错误
    /// 无权限或读取失败时返回错误。
    pub async fn selected(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        ids: Vec<String>,
    ) -> Result<PersonDirectoryPage> {
        self.read(actor, category, Read::Selected(ids)).await
    }

    async fn read(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        read: Read,
    ) -> Result<PersonDirectoryPage> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                let db = db.clone();
                let rbac = rbac.clone();
                let actor = actor.clone();
                let read = read.clone();
                Box::pin(async move {
                    match read {
                        Read::List(request) => {
                            query::list_page(&db, &rbac, &actor, category, &request, executor).await
                        },
                        Read::Selected(ids) => {
                            query::selected_page(&db, &rbac, &actor, category, &ids, executor).await
                        },
                    }
                })
            })
            .await
    }
}

#[derive(Clone)]
enum Read {
    List(DirectoryListRequest),
    Selected(Vec<String>),
}
