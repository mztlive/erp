//! 销售与采购人员查询目录。页面只调用这里，不各自拼账号或业务记录。

mod grant;
pub mod management;
mod query;
mod sync;

use std::sync::Arc;

use application_core::AuditActor;
pub use grant::grant_assigned_roles;
use mongodb::Database;
use persistence_core::Transactional;
pub use sync::sync_role_grants;

use crate::dto::{PersonDirectoryPage, PersonDirectoryQuery};
use crate::entity::person_directory::{DirectoryListRequest, PersonDirectoryCategory};
use crate::{AuthorizationService, Result};

/// 提供后台、销售和采购人员的授权列表、已选回显和岗位查询资格管理。
pub struct PersonDirectoryService {
    // 人员档案、组织及目录资格所在的应用数据库。
    db: Database,
    // 共享授权服务，统一生成目录过滤并核验管理目标权限。
    authorization: Arc<AuthorizationService>,
}

impl PersonDirectoryService {
    /// 绑定身份数据库和共享授权服务，供列表、回显和资格维护共用。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 按固定目录类别查询分页列表。授权、岗位资格和组织事实使用同一事务；调用方可指定目录自身的搜索和组织筛选。
    pub async fn list(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        query: PersonDirectoryQuery,
    ) -> Result<PersonDirectoryPage> {
        let request = query.into_request()?;
        self.read(actor, category, Read::List(request)).await
    }

    /// 按已选账号 ID 回显当前仍可读的人员，适用于重新打开选择器。只返回仍满足目录资格和权限的账号。
    pub async fn selected(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        ids: Vec<String>,
    ) -> Result<PersonDirectoryPage> {
        self.read(actor, category, Read::Selected(ids)).await
    }

    // 为列表或回显开启读取事务，并分派到对应的目录查询，使授权和展示使用同一快照。
    async fn read(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        read: Read,
    ) -> Result<PersonDirectoryPage> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    match read {
                        Read::List(request) => {
                            query::list_page(&db, &authorization, &actor, category, &request, executor).await
                        },
                        Read::Selected(ids) => {
                            query::selected_page(&db, &authorization, &actor, category, &ids, executor).await
                        },
                    }
                })
            })
            .await
    }
}

enum Read {
    List(DirectoryListRequest),
    Selected(Vec<String>),
}
