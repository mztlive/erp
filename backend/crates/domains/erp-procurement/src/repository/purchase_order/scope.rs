//! 采购单读取仅消费 identity 生成的完整查询凭证。

use std::sync::Arc;

use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, aggregate};

use super::PURCHASE_ORDERS;

/// 当前读取阶段的统一授权凭证，未装配时保持空集。
#[derive(Clone, Default)]
pub struct PurchaseReadScope {
    access: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
}

impl std::fmt::Debug for PurchaseReadScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PurchaseReadScope").field("access", &self.access).finish()
    }
}

impl PurchaseReadScope {
    /// 绑定统一授权服务及本阶段编译结果。
    /// # 参数
    /// access 是 identity 编译结果；authorization 是生成它的服务。
    /// # 返回
    /// 不缓存跨请求授权的仓储读取凭证。
    /// # 错误
    /// 不执行 I/O。
    pub fn from_compiled(access: Arc<CompiledAccess>, authorization: AuthorizationService) -> Self {
        Self { access: Some(access), authorization: Some(authorization) }
    }

    /// 读取当前授权与业务映射版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已编译版本；无凭证时为空。
    /// # 错误
    /// 不执行 I/O。
    pub fn version(&self) -> Option<String> {
        self.access.as_ref().map(|access| access.version())
    }

    /// 返回完整授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前阶段的只读聚合条件。
    /// # 错误
    /// 不执行 I/O。
    pub fn stages(&self) -> &[Document] {
        self.access.as_ref().map_or(&[], |access| access.stages())
    }

    /// 在原执行器复验凭证。
    /// # 参数
    /// executor 是本次读取的执行器。
    /// # 返回
    /// 身份、授权与业务依赖未变时成功。
    /// # 错误
    /// 失效授权、事实版本变化或设施失败时返回错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        if let (Some(access), Some(service)) = (&self.access, &self.authorization) {
            service.validate(access, executor).await?;
        }
        Ok(())
    }

    /// 将业务筛选放在完整授权阶段之前。
    /// # 参数
    /// filter 只包含业务筛选。
    /// # 返回
    /// 没有分页的候选与授权阶段。
    /// # 错误
    /// 不执行 I/O。
    pub fn lookup_after(&self, filter: Document) -> Vec<Document> {
        let mut stages = vec![doc! { "$match": filter }];
        stages.extend_from_slice(self.stages());
        stages
    }

    /// 沿关联单据的实际来源采购单执行完整授权。
    ///
    /// # 参数
    /// `filter` 是关联单据业务筛选；`source_id` 是仓储固定的来源 ID 字段表达式。
    /// # 返回
    /// 只保留来源存在且通过统一授权的关联单据，不展开或复制根记录。
    /// # 错误
    /// 不执行 I/O；缺少凭证时返回空集条件。
    pub fn related_stages(&self, filter: Document, source_id: &str) -> Vec<Document> {
        related_pipeline(filter, source_id, self.access.as_ref().map(|access| access.stages()))
    }

    /// 未取得统一授权凭证时保持空集。
    /// # 参数
    /// 无。
    /// # 返回
    /// 缺少凭证时为 true。
    /// # 错误
    /// 无。
    pub fn is_empty(&self) -> bool {
        self.access.is_none()
    }

    /// 凭证已在阶段中执行；缺少凭证时拒绝全部记录。
    /// # 参数
    /// 无。
    /// # 返回
    /// 仓储最终候选条件。
    /// # 错误
    /// 无。
    pub fn document(&self) -> Document {
        if self.access.is_some() {
            doc! {}
        } else {
            doc! { "$expr": false }
        }
    }
}

/// 授权在单条实际来源连接内执行，不枚举账号全部可见采购单。
fn related_pipeline(filter: Document, source_id: &str, access: Option<&[Document]>) -> Vec<Document> {
    let mut stages = vec![doc! { "$match": filter }];
    let Some(access) = access else {
        stages.push(doc! { "$match": { "$expr": false } });
        return stages;
    };
    let mut source = vec![doc! { "$match": { "deleted_at": 0, "$expr": { "$eq": ["$id", "$$source_id"] } } }];
    source.extend_from_slice(access);
    source.extend([doc! { "$limit": 1 }, doc! { "$project": { "_id": 0, "id": 1 } }]);
    stages.extend([
        doc! { "$lookup": { "from": PURCHASE_ORDERS, "let": { "source_id": source_id }, "pipeline": source, "as": "__authorized_purchase_source" } },
        doc! { "$match": { "__authorized_purchase_source.0": { "$exists": true } } },
        doc! { "$unset": "__authorized_purchase_source" },
    ]);
    stages
}

/// 采购主表授权范围查询。
#[allow(async_fn_in_trait)]
pub trait PurchaseOrderRepositoryScopeExt {
    /// 按独立授权条件读取采购单，ID 条件不能替换范围交集。
    ///
    /// # 参数
    /// * `id` - 采购单稳定主键
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 None。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 仓储不得按登录用户自行推断权限。
    async fn find_authorized(
        &self,
        id: &str,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Option<crate::entity::purchase_order::PurchaseOrder>>;

    /// 装载查询的有界身份与版本集合，用于跨页及导出的一致性校验。
    ///
    /// # 参数
    /// * `filter` - 业务筛选
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行；调用方必须整体拒绝超限，不得截断版本集合。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 版本集合必须与列表同一授权条件和筛选快照。
    async fn query_versions(
        &self,
        filter: &super::PurchaseOrderFilter,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Vec<PurchaseVersion>>;
}

impl PurchaseOrderRepositoryScopeExt
    for persistence_core::Repository<'_, crate::entity::purchase_order::PurchaseOrder>
{
    async fn find_authorized(
        &self,
        id: &str,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Option<crate::entity::purchase_order::PurchaseOrder>> {
        scope.validate(executor).await?;
        let rows = aggregate::find(
            &self.collection(),
            doc! { "deleted_at": 0, "$and": [{ "id": id }, scope.document()] },
            FindOptions::builder().limit(1).build(),
            scope.lookup_after(doc! { "deleted_at": 0, "id": id }),
            executor,
        )
        .await?;
        scope.validate(executor).await?;
        Ok(rows.into_iter().next())
    }

    async fn query_versions(
        &self,
        filter: &super::PurchaseOrderFilter,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Vec<PurchaseVersion>> {
        scope.validate(executor).await?;
        let rows = aggregate::find(
            &self.collection().clone_with_type::<PurchaseVersion>(),
            doc! { "$and": [filter.to_doc(), scope.document()] },
            mongodb::options::FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            scope.lookup_after(filter.to_doc()),
            executor,
        )
        .await?;
        scope.validate(executor).await?;
        Ok(rows)
    }
}

/// 跨页校验使用的业务身份和版本，不包含金额及其他展示字段。
#[derive(Debug, serde::Deserialize, Hash)]
pub struct PurchaseVersion {
    /// 采购单稳定主键。
    pub id: String,
    /// 采购单乐观锁版本。
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, doc};

    use super::{PurchaseReadScope, related_pipeline};

    #[test]
    fn related_query_keeps_business_filter_and_complete_correlated_authorization() {
        let filter = doc! { "deleted_at": 0, "purchase_order_id": "po-requested", "status": "draft" };
        let access = vec![
            doc! { "$lookup": { "from": "resource_grants", "as": "grant", "pipeline": [] } },
            doc! { "$match": { "$or": [{ "current_owner_user_id": "owner-1" }, { "grant.0": { "$exists": true } }] } },
            doc! { "$unset": "grant" },
        ];
        let stages = related_pipeline(filter.clone(), "$purchase_order_id", Some(&access));
        assert_eq!(stages[0], doc! { "$match": filter });
        let lookup = stages[1].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_document("let").unwrap(), &doc! { "source_id": "$purchase_order_id" });
        let source = lookup.get_array("pipeline").unwrap();
        assert_eq!(
            source[0],
            Bson::Document(
                doc! { "$match": { "deleted_at": 0, "$expr": { "$eq": ["$id", "$$source_id"] } } }
            )
        );
        assert_eq!(&source[1..=access.len()], access.iter().cloned().map(Bson::Document).collect::<Vec<_>>());
        assert_eq!(source[access.len() + 1], Bson::Document(doc! { "$limit": 1 }));
        assert_eq!(stages[2], doc! { "$match": { "__authorized_purchase_source.0": { "$exists": true } } });
        assert_eq!(stages[3], doc! { "$unset": "__authorized_purchase_source" });
    }

    #[test]
    fn related_query_without_authorization_proof_is_empty() {
        let scope = PurchaseReadScope::default();
        assert_eq!(
            scope.related_stages(doc! { "purchase_order_id": "po-1" }, "$purchase_order_id"),
            vec![doc! { "$match": { "purchase_order_id": "po-1" } }, doc! { "$match": { "$expr": false } },]
        );
    }
}
