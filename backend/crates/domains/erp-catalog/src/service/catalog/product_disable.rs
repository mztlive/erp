//! 停用商品：范围重验、不可变修订与审计。

use application_core::AuditActor;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;
use validator::Validate;

use super::support::ensure_version;
use super::{CatalogAccess, CatalogService};
use crate::dto::{DisableProductRequest, ProductView};
use crate::entity::catalog::product::Product;
use crate::entity::catalog::{ProductRevisionId, ProductRevisionMediaId, next_revision_no};
use crate::error::{Error, Result};
use crate::ports::{CatalogAuditPort, PreparedCatalogAudit};
use crate::repository::CatalogExt;

impl CatalogService {
    /// 停用商品并生成一份服务端派生的不可变商品修订。
    ///
    /// 客户端只提交商品身份、已见版本、原因和生效日。服务端在同一事务内
    /// 重验维护范围、复制当前媒体、写入停用修订并记录审计。
    ///
    /// # 参数
    /// * `id` - 商品稳定 ID
    /// * `req` - 停用命令
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回停用后的商品视图。
    ///
    /// # 错误
    /// 对象不可见、缺少维护责任、版本冲突或已经停用时拒绝。
    ///
    /// # 关键业务约束
    /// 写命令在原事务内按 `product:update` 重验；空维护人须先交接。
    pub async fn product_disable(
        &self,
        id: &str,
        req: DisableProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "product.disable",
            "product",
            id.to_string(),
            req.change_reason.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let access = self.access();
        let actor = actor.clone();
        let id = id.to_string();
        let updated = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    write_disabled_product(
                        DisableWrite {
                            db: &db,
                            access: &access,
                            audit_port: audit_port.as_ref(),
                            id: &id,
                            req: &req,
                            actor: &actor,
                            audit: &audit,
                        },
                        executor,
                    )
                    .await
                })
            })
            .await?;
        self.product_view(updated).await
    }
}

struct DisableWrite<'a> {
    db: &'a Database,
    access: &'a CatalogAccess,
    audit_port: &'a dyn CatalogAuditPort,
    id: &'a str,
    req: &'a DisableProductRequest,
    actor: &'a AuditActor,
    audit: &'a PreparedCatalogAudit,
}

/// 在调用方事务内重验范围并写入停用修订。
///
/// # 参数
/// * `input` - 停用写入所需的数据库、范围、审计与命令
/// * `session` - 调用方执行器
///
/// # 返回
/// 返回停用后的商品实体。
///
/// # 错误
/// 不可见、缺责任、版本冲突或已经停用时整事务回滚。
async fn write_disabled_product(
    input: DisableWrite<'_>,
    session: &mut dyn persistence_core::Executor,
) -> Result<Product> {
    let DisableWrite { db, access, audit_port, id, req, actor, audit } = input;
    let scoped = access.require_product(actor, "update", id, session).await?;
    scoped.ensure_has_responsibility()?;
    ensure_version(scoped.base.version, req.version)?;
    let snapshot = db
        .catalog()
        .product_disable_snapshot(id, session)
        .await?
        .ok_or_else(|| Error::NotFound("商品不存在或无权查看".to_string()))?;
    let mut product = snapshot.product;
    product.disable(actor.id()).map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let revision_no = next_revision_no(snapshot.latest_revision_no)?;
    let current_revision =
        snapshot.current_revision.ok_or_else(|| Error::NotFound("商品当前修订不存在".to_string()))?;
    let revision = current_revision.disabled_successor(
        ProductRevisionId::new(next_id()),
        revision_no,
        req.effective_from,
    )?;
    let media = snapshot
        .media
        .iter()
        .map(|row| {
            row.copy_to_revision(
                ProductRevisionMediaId::new(next_id()),
                ProductRevisionId::new(revision.base.id.clone()),
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    product.attach_revision(&revision, actor.id())?;
    db.products().update(&mut product, session).await?;
    db.catalog().create_product_revision_with_media(&revision, &media, session).await?;
    audit_port.persist(audit, session).await?;
    Ok(product)
}
