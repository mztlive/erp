//! 域 D09 `supplier_rating_revision` 服务编排。
//!
//! 期初评分只在首次合作版本填写；合作中评分与评级按周期追加新版本，
//! 不原位覆盖（§6.2）。同一供应商评估版本有效期不得重叠（§6.2 跨行约束，
//! 事务内校验）。本集合是纯追加式不可变修订（无表头实体）。

use database::{AccessControlExt, NoTransaction, SupplierExt, Transactional};
use entities::common::revision::RevisionBase;
use entities::common::time::BusinessDate;
use entities::ids::SupplierAccountId;
use entities::supplier::{SupplierRatingRevision, SupplierRatingRevisionData, SupplierRatingRevisionId};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, CreateSupplierRatingRequest, PageView, SortDir, SupplierRatingListParams,
    SupplierRatingView, RATING_SORT_FIELDS,
};
use super::{page_or_default, page_size_or_default};

/// 供应商评估版本服务。
pub struct SupplierRatingService {
    db: Database,
}

impl SupplierRatingService {
    /// 创建供应商评估版本服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商评估版本列表。
    ///
    /// 仓储层未提供评估版本的分页投影入口（只有基类查询），且评估版本按
    /// 供应商追加、行数天然受限；此处按供应商全量读取后内存分页，`total`
    /// 为满足条件的真实总数。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn supplier_rating_list(
        &self,
        supplier_id: &str,
        params: &SupplierRatingListParams,
    ) -> Result<PageView<SupplierRatingView>> {
        params.validate()?;
        let (sort_by, sort_dir) = normalize_sort(&params.sort_by, &params.sort_dir, RATING_SORT_FIELDS)?;
        let all = self
            .db
            .supplier_rating_revisions()
            .find_many_sorted(
                doc! { "supplier_id": supplier_id },
                sort_doc(sort_by, sort_dir),
                &mut NoTransaction,
            )
            .await?;
        let total = all.len() as i64;
        let page = page_or_default(params.page);
        let page_size = page_size_or_default(params.page_size);
        let offset = ((page - 1) * u64::from(page_size)) as usize;
        let items = all
            .into_iter()
            .skip(offset)
            .take(page_size as usize)
            .map(Into::into)
            .collect();

        Ok(PageView {
            items,
            total,
            page,
            page_size,
        })
    }

    /// 创建供应商评估版本（跨集合事务：版本 + 审计原子写入）。
    ///
    /// 版本号按既有评估版本自动递增；期初评分只在首次版本允许填写
    /// （§6.2，实体校验）；同一供应商评估版本有效期不得重叠（事务内校验）。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建评估版本的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    /// * `ConflictError` - 版本区间与既有版本重叠
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_supplier_rating(
        &self,
        supplier_id: &str,
        req: CreateSupplierRatingRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRatingView> {
        req.validate()?;
        self.ensure_supplier_exists(supplier_id).await?;
        let audit = actor.clone().resource_log(
            "supplier_rating.create",
            "supplier_rating",
            supplier_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let supplier_id_for_tx = SupplierAccountId::new(supplier_id);
        let revision_for_tx = SupplierRatingRevision::new(
            SupplierRatingRevisionId::new(next_id()),
            SupplierRatingRevisionData {
                supplier_id: supplier_id_for_tx.clone(),
                revision_no: 0,
                initial_score: req.initial_score,
                rating: req.rating,
                current_score: req.current_score,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                change_reason: req.change_reason,
            },
        )?;
        let effective_from = req.valid_from;
        let effective_to = req.valid_to;
        let revision = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let history = db
                        .supplier_rating_revisions()
                        .find_many(doc! { "supplier_id": supplier_id_for_tx.to_string() }, session)
                        .await?;
                    let next_no = history
                        .iter()
                        .map(|revision| revision.revision.revision_no)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    ensure_no_window_overlap(&history, effective_from, effective_to)?;
                    let revision = SupplierRatingRevision {
                        revision: RevisionBase::new(next_no),
                        ..revision_for_tx
                    };
                    db.supplier_rating_revisions().create(&revision, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierRatingRevision, crate::errors::Error>(revision)
                })
            })
            .await?;

        Ok(revision.into())
    }

    /// 校验供应商角色存在。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    ///
    /// # 返回
    /// 供应商存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    async fn ensure_supplier_exists(&self, supplier_id: &str) -> Result<()> {
        self.db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        Ok(())
    }
}

/// 构建排序文档（仓储白名单语义与 repository 层一致）。
///
/// # 参数
/// * `sort_by` - 已过白名单校验的排序字段
/// * `sort_dir` - 排序方向
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: &'static str, sort_dir: SortDir) -> mongodb::bson::Document {
    let direction = if matches!(sort_dir, SortDir::Asc) { 1 } else { -1 };
    doc! { sort_by: direction }
}
/// 校验新评估版本窗口与既有版本区间不重叠（§6.2）。
///
/// # 参数
/// * `history` - 既有评估版本
/// * `effective_from` - 新版本生效开始
/// * `effective_to` - 新版本生效结束（`None` 表示长期有效）
///
/// # 返回
/// 无重叠返回 `Ok(())`。
///
/// # 错误
/// 与任一既有版本区间重叠时返回 `ConflictError`。
fn ensure_no_window_overlap(
    history: &[SupplierRatingRevision],
    effective_from: BusinessDate,
    effective_to: Option<BusinessDate>,
) -> Result<()> {
    for existing in history {
        let a_covers = |day: BusinessDate| effective_to.is_none_or(|end| day <= end);
        let b_covers = |day: BusinessDate| existing.valid_to.is_none_or(|end| day <= end);
        if a_covers(existing.valid_from) && b_covers(effective_from) {
            return Err(Error::ConflictError(
                "评估版本生效区间与既有版本重叠，请调整生效日期".to_string(),
            ));
        }
    }
    Ok(())
}
