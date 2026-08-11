use database::{AccessControlExt, CatalogExt, NoTransaction, Transactional};
use entities::catalog::product::{Product, ProductData};
use entities::catalog::product_category::{ProductCategory, ProductCategoryData};
use entities::catalog::product_revision::{ProductRevision, ProductRevisionData};
use entities::catalog::sku::Sku;
use entities::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use entities::catalog::voucher_category_profile_revision::{
    VoucherCategoryProfileRevision, VoucherCategoryProfileRevisionData,
};
use entities::catalog::{
    EnableStatus, ProductCategoryId, ProductId, ProductKind, ProductRevisionId, SkuId, SkuRevisionId,
    VoucherCategoryProfileRevisionId,
};
use entities::common::time::BusinessDate;
use id_generator::next_id;
use mongodb::bson::doc;
use validator::Validate;

use super::sku_edit::{NewSkuContext, SkuEditItem};
use super::support::ensure_version;
use super::CatalogService;
use crate::audit::AuditActor;
use crate::catalog::dto::{
    CreateVoucherCategoryRequest, NewVoucherCategoryInput, PageView, ProductSkuInput, SortDir,
    UpdateVoucherCategoryRequest, VoucherCategoryProfileListParams, VoucherCategoryProfileView,
    VoucherSkuInput,
};
use crate::errors::{Error, Result};

/// 卡券类目扩展修订列表筛选条件类型。
type VoucherProfileFilter = <mongodb::Database as CatalogExt>::VoucherCategoryProfileRevisionFilter;

/// 卡券类目原子创建草稿（全部 ID 在事务外预生成，事务内只做写入）。
struct VoucherCategoryDraft {
    /// 内联新建的分类（引用已有分类时为 `None`）。
    new_category: Option<ProductCategory>,
    /// SPU 稳定身份。
    product: Product,
    /// 商品修订快照。
    revision: ProductRevision,
    /// 唯一 SKU 行（action 恒为 `Create`）。
    sku_item: SkuEditItem,
    /// 卡券类目扩展修订。
    voucher_revision: VoucherCategoryProfileRevision,
}

/// 卡券类目更新草稿（更新 SPU 指针 + 追加商品/SKU/扩展修订）。
struct VoucherCategoryUpdateDraft {
    /// 已更新 `current_revision_id` 的商品。
    product: Product,
    /// 新商品修订。
    product_revision: ProductRevision,
    /// 已更新 `current_revision_id` 的 SKU。
    sku: Sku,
    /// 新 SKU 修订。
    sku_revision: SkuRevision,
    /// 新卡券类目扩展修订。
    voucher_revision: VoucherCategoryProfileRevision,
}

impl CatalogService {
    // ---------- 卡券类目扩展 ----------

    /// 分页查询卡券类目扩展修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sku_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn voucher_category_profile_list(
        &self,
        params: &VoucherCategoryProfileListParams,
    ) -> Result<PageView<VoucherCategoryProfileView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = VoucherProfileFilter {
            sku_id: query.sku_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .voucher_category_profile_revisions()
            .search_voucher_category_profile_revisions(&filter, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let mut view = VoucherCategoryProfileView {
                id: row.id,
                sku_id: row.sku_id,
                sku_no: None,
                product_id: None,
                product_version: None,
                name: None,
                revision_no: row.revision_no,
                description: row.description,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            };
            self.enrich_voucher_category_view(&mut view).await?;
            items.push(view);
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 更新卡券类目名称与描述（按 SKU 稳定身份定位，追加商品/SKU/扩展修订）。
    ///
    /// 分类、品牌、基础单位与编号保持不变；乐观锁使用所属商品 `product.version`。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目对应的 VOUCHER SKU 稳定 ID
    /// * `req` - 更新请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的卡券类目扩展修订视图（含名称与商品版本）。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - SKU/商品/当前修订不存在，或 SKU 非 VOUCHER 商品
    /// * `ConflictError` - 商品乐观锁冲突或并发写入冲突
    pub async fn voucher_category_update(
        &self,
        sku_id: &str,
        req: UpdateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileView> {
        req.validate()?;
        let draft = self
            .build_voucher_category_update_draft(sku_id, req, actor)
            .await?;
        let revision = self.write_voucher_category_update_draft(draft, actor).await?;
        let mut view = VoucherCategoryProfileView::from(revision);
        self.enrich_voucher_category_view(&mut view).await?;
        Ok(view)
    }

    /// 原子创建卡券类目（商品 + 首个修订 + 唯一 SKU + [可选内联新建分类] +
    /// 卡券类目扩展修订，同一事务写入）。
    ///
    /// 业务上一个卡券类目即一个 VOUCHER 类型的 SKU：`voucher_no` 同时作为
    /// `product_no` 与 `sku_no`。分类 / 品牌 / 基础单位可省略：
    /// - 分类：缺省挂到共用卡券根分类（代码 `VOUCHER`）；
    /// - 品牌：缺省「福尚云」；
    /// - 基础单位：缺省「张」。
    ///   上述默认字典不存在时由本方法自动创建（单集合写入，位于事务外）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建卡券类目扩展修订的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败，或分类与内联新建同时给出
    /// * `NotFound` - 显式引用的分类/品牌/基础单位不存在
    /// * `BusinessLogicError` - 分类不允许 VOUCHER 类型、父子关系成环、基础单位已停用
    /// * `ConflictError` - `product_no`/`sku_no`/`category_code`/条码 唯一约束冲突
    pub async fn voucher_category_create(
        &self,
        req: CreateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileView> {
        req.validate()?;
        let draft = self.build_voucher_category_draft(req, actor).await?;
        let revision = self.write_voucher_category_draft(draft, actor).await?;
        let mut view = VoucherCategoryProfileView::from(revision);
        self.enrich_voucher_category_view(&mut view).await?;
        Ok(view)
    }

    /// 构造卡券类目原子创建草稿（分类二选一 + 品牌/单位校验 + 商品/SKU/卡券
    /// 修订预生成，全部 ID 在事务外预生成，事务内只做写入）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回待写入的草稿。
    ///
    /// # 错误
    /// * `ValidationError` - `category_id` 与 `new_category` 同时给出
    /// * `NotFound` - 显式引用的分类/父分类/品牌/基础单位不存在
    /// * `BusinessLogicError` - 分类不允许 VOUCHER 类型、父子关系成环、基础单位已停用、条码冲突
    async fn build_voucher_category_draft(
        &self,
        req: CreateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryDraft> {
        let CreateVoucherCategoryRequest {
            voucher_no,
            name,
            description,
            specification,
            category_id,
            new_category,
            brand_id,
            sku,
            status,
            effective_from,
            effective_to,
        } = req;

        ensure_category_selection_exclusive(&category_id, &new_category)?;
        let (category_id, new_category) = match (category_id, new_category) {
            (Some(category_id), None) => {
                let category = self.load_category(category_id.as_ref()).await?;
                if category.product_kind != ProductKind::Voucher {
                    return Err(Error::BusinessLogicError(
                        "所选分类不允许 VOUCHER 类型".to_string(),
                    ));
                }
                (category_id, None)
            }
            (None, Some(new_category_input)) => {
                let new_category_id = ProductCategoryId::new(next_id());
                self.ensure_parent_chain_ok(
                    new_category_id.as_ref(),
                    new_category_input.parent_category_id.as_ref(),
                )
                .await?;
                let category = ProductCategory::new(
                    new_category_id.clone(),
                    ProductCategoryData {
                        category_code: new_category_input.category_code,
                        parent_category_id: new_category_input.parent_category_id,
                        name: new_category_input.name,
                        product_kind: ProductKind::Voucher,
                        status: EnableStatus::Active,
                    },
                    actor.id(),
                )?;
                (new_category_id, Some(category))
            }
            (None, None) => {
                let root_id = self.ensure_voucher_root_category(actor).await?;
                (root_id, None)
            }
            (Some(_), Some(_)) => {
                unreachable!("ensure_category_selection_exclusive 已拒绝同时给出")
            }
        };

        let brand_id = match brand_id {
            Some(brand_id) => {
                self.load_brand(brand_id.as_ref()).await?;
                brand_id
            }
            None => self.ensure_voucher_default_brand(actor).await?,
        };

        let sku = match sku {
            Some(sku) => sku,
            None => VoucherSkuInput {
                base_unit_id: self.ensure_voucher_default_unit(actor).await?,
                barcode: None,
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: None,
                market_price: None,
            },
        };

        self.ensure_brand_and_unit_ok(&brand_id, std::iter::once(&sku.base_unit_id))
            .await?;

        let effective_from = effective_from.unwrap_or_else(BusinessDate::today);
        let product_id = ProductId::new(next_id());
        let revision_id = ProductRevisionId::new(next_id());
        let product_status = status.unwrap_or(EnableStatus::Active);
        let mut product = Product::new(
            product_id.clone(),
            ProductData {
                product_no: voucher_no.clone(),
                product_kind: ProductKind::Voucher,
                status: product_status,
            },
            actor.id(),
        )?;

        let sku_item = self
            .build_new_sku_item(
                NewSkuContext {
                    product_id: &product_id,
                    product_name: &name,
                    effective_from,
                    effective_to,
                    created_by: actor.id(),
                },
                ProductSkuInput {
                    sku_id: None,
                    expected_sku_revision_id: None,
                    reenable: false,
                    sku_no: voucher_no,
                    base_unit_id: sku.base_unit_id,
                    barcode: sku.barcode,
                    main_image_asset_id: None,
                    weight_kg: sku.weight_kg,
                    volume_m3: sku.volume_m3,
                    sales_visible_price_gross: sku.sales_visible_price_gross,
                    market_price: sku.market_price,
                    spec_entries: Vec::new(),
                },
            )
            .await?;

        let revision = ProductRevision::new(
            revision_id.clone(),
            ProductRevisionData {
                product_id,
                revision_no: 1,
                name,
                description: Some(description.clone()),
                specification,
                category_id,
                brand_id,
                status: product_status,
                effective_from,
                effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(revision.base.id.clone());

        let voucher_revision = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new(next_id()),
            VoucherCategoryProfileRevisionData {
                sku_id: SkuId::new(sku_item.sku.base.id.clone()),
                revision_no: 1,
                description,
                status: product_status,
            },
        )?;

        Ok(VoucherCategoryDraft {
            new_category,
            product,
            revision,
            sku_item,
            voucher_revision,
        })
    }
    /// 在单个事务内写入卡券类目创建草稿并返回卡券类目扩展修订。
    ///
    /// 依次写入 `[新建分类]`、`products`、`product_revisions`（无媒体）、
    /// 唯一 SKU 的 `skus` + `sku_revisions`、`voucher_category_profile_revisions`，
    /// 以及审计日志。
    ///
    /// # 参数
    /// * `draft` - 创建草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回写入后的卡券类目扩展修订实体。
    ///
    /// # 错误
    /// 唯一索引冲突（409）或事务失败时返回错误并整体回滚。
    async fn write_voucher_category_draft(
        &self,
        draft: VoucherCategoryDraft,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileRevision> {
        let audit = actor.clone().resource_log(
            "voucher_category.create",
            "voucher_category_profile",
            draft.voucher_revision.base.id.clone(),
        )?;
        let VoucherCategoryDraft {
            new_category,
            product,
            revision,
            sku_item,
            voucher_revision,
        } = draft;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(category) = &new_category {
                        db.product_categories().create(category, session).await?;
                    }
                    db.products().create(&product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &[], session)
                        .await?;
                    db.catalog()
                        .create_sku_with_revision(&sku_item.sku, &sku_item.revision, &[], session)
                        .await?;
                    db.voucher_category_profile_revisions()
                        .create(&voucher_revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<VoucherCategoryProfileRevision, crate::errors::Error>(voucher_revision)
                })
            })
            .await
    }

    /// 构造卡券类目更新草稿：沿用分类/品牌/单位与价格，仅改名称与描述。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU 稳定 ID
    /// * `req` - 更新请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回待写入的更新草稿。
    ///
    /// # 错误
    /// SKU/商品/当前修订不存在、非 VOUCHER 商品或乐观锁失败时返回错误。
    async fn build_voucher_category_update_draft(
        &self,
        sku_id: &str,
        req: UpdateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryUpdateDraft> {
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡券类目 SKU 不存在".to_string()))?;
        let mut product = self.load_product(sku.product_id.as_ref()).await?;
        if product.product_kind != ProductKind::Voucher {
            return Err(Error::BusinessLogicError(
                "目标 SKU 不属于卡券类目商品".to_string(),
            ));
        }
        ensure_version(product.base.version, req.version)?;

        let current_product_revision = self.load_current_product_revision(&product).await?;
        let current_sku_revision = self.load_current_sku_revision(sku_id).await?;

        let effective_from = req.effective_from.unwrap_or_else(BusinessDate::today);
        let effective_to = req.effective_to;
        let product_status = product.stable.status;
        let next_product_revision_no = self.next_product_revision_no(product.base.id.as_str()).await?;
        let next_sku_revision_no = self.next_sku_revision_no(sku_id).await?;
        let next_voucher_revision_no = self.next_voucher_profile_revision_no(sku_id).await?;

        let product_revision = ProductRevision::new(
            ProductRevisionId::new(next_id()),
            ProductRevisionData {
                product_id: ProductId::new(product.base.id.clone()),
                revision_no: next_product_revision_no,
                name: req.name.clone(),
                description: Some(req.description.clone()),
                specification: current_product_revision.specification.clone(),
                category_id: current_product_revision.category_id.clone(),
                brand_id: current_product_revision.brand_id.clone(),
                status: product_status,
                effective_from,
                effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(product_revision.base.id.clone());
        product.stable.touch(actor.id());

        let sku_revision = SkuRevision::new(
            SkuRevisionId::new(next_id()),
            SkuRevisionData {
                sku_id: SkuId::new(sku_id.to_string()),
                revision_no: next_sku_revision_no,
                name: req.name,
                description: Some(req.description.clone()),
                specification: current_sku_revision.specification.clone(),
                barcode: current_sku_revision.barcode.clone(),
                source_main_image_asset_id: current_sku_revision.source_main_image_asset_id.clone(),
                weight_kg: current_sku_revision.weight_kg,
                volume_m3: current_sku_revision.volume_m3,
                sales_visible_price_gross: current_sku_revision.sales_visible_price_gross,
                market_price: current_sku_revision.market_price,
                status: current_sku_revision.status,
                effective_from,
                effective_to,
            },
        )?;
        let mut sku = sku;
        sku.stable.current_revision_id = Some(sku_revision.base.id.clone());
        sku.stable.touch(actor.id());

        let voucher_revision = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new(next_id()),
            VoucherCategoryProfileRevisionData {
                sku_id: SkuId::new(sku_id.to_string()),
                revision_no: next_voucher_revision_no,
                description: req.description,
                status: product_status,
            },
        )?;

        Ok(VoucherCategoryUpdateDraft {
            product,
            product_revision,
            sku,
            sku_revision,
            voucher_revision,
        })
    }

    /// 在事务内写入卡券类目更新草稿。
    ///
    /// # 参数
    /// * `draft` - 更新草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新建的卡券类目扩展修订。
    ///
    /// # 错误
    /// 乐观锁冲突或事务失败时返回错误并整体回滚。
    async fn write_voucher_category_update_draft(
        &self,
        draft: VoucherCategoryUpdateDraft,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileRevision> {
        let audit = actor.clone().resource_log(
            "voucher_category.update",
            "voucher_category_profile",
            draft.voucher_revision.base.id.clone(),
        )?;
        let VoucherCategoryUpdateDraft {
            mut product,
            product_revision,
            mut sku,
            sku_revision,
            voucher_revision,
        } = draft;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().update(&mut product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&product_revision, &[], session)
                        .await?;
                    db.skus().update(&mut sku, session).await?;
                    db.sku_revisions().create(&sku_revision, session).await?;
                    db.voucher_category_profile_revisions()
                        .create(&voucher_revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<VoucherCategoryProfileRevision, crate::errors::Error>(voucher_revision)
                })
            })
            .await
    }

    /// 为卡券类目视图补齐 SKU 编号、商品版本与展示名称。
    ///
    /// # 参数
    /// * `view` - 待补齐的响应视图（`sku_id` 必须已填）
    ///
    /// # 返回
    /// 成功时返回 `Ok(())`；关联数据缺失时静默跳过补齐字段。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn enrich_voucher_category_view(&self, view: &mut VoucherCategoryProfileView) -> Result<()> {
        let Some(sku) = self
            .db
            .skus()
            .find_by_id(&view.sku_id, &mut NoTransaction)
            .await?
        else {
            return Ok(());
        };
        view.sku_no = Some(sku.sku_no.clone());
        view.product_id = Some(sku.product_id.to_string());
        if let Ok(product) = self.load_product(sku.product_id.as_ref()).await {
            view.product_version = Some(product.base.version);
            if let Ok(revision) = self.load_current_product_revision(&product).await {
                view.name = Some(revision.name);
            }
        }
        if view.name.is_none() {
            if let Ok(sku_revision) = self.load_current_sku_revision(&view.sku_id).await {
                view.name = Some(sku_revision.name);
            }
        }
        if view.name.is_none() {
            view.name = Some(view.description.clone());
        }
        Ok(())
    }

    /// 加载商品当前修订（优先 `current_revision_id`，否则取最大修订号）。
    ///
    /// # 参数
    /// * `product` - 已加载的商品
    ///
    /// # 返回
    /// 返回当前商品修订。
    ///
    /// # 错误
    /// 不存在任何修订时返回 `NotFound`。
    async fn load_current_product_revision(&self, product: &Product) -> Result<ProductRevision> {
        if let Some(revision_id) = product.stable.current_revision_id.as_ref() {
            if let Some(revision) = self
                .db
                .product_revisions()
                .find_by_id(revision_id, &mut NoTransaction)
                .await?
            {
                return Ok(revision);
            }
        }
        let revisions = self
            .db
            .product_revisions()
            .find_many(doc! { "product_id": product.base.id.clone() }, &mut NoTransaction)
            .await?;
        revisions
            .into_iter()
            .max_by_key(|revision| revision.revision.revision_no)
            .ok_or_else(|| Error::NotFound("商品修订不存在".to_string()))
    }

    /// 加载 SKU 当前修订（优先 `current_revision_id`，否则取最大修订号）。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    ///
    /// # 返回
    /// 返回当前 SKU 修订。
    ///
    /// # 错误
    /// SKU 或修订不存在时返回 `NotFound`。
    async fn load_current_sku_revision(&self, sku_id: &str) -> Result<SkuRevision> {
        if let Some(sku) = self.db.skus().find_by_id(sku_id, &mut NoTransaction).await? {
            if let Some(revision_id) = sku.stable.current_revision_id.as_ref() {
                if let Some(revision) = self
                    .db
                    .sku_revisions()
                    .find_by_id(revision_id, &mut NoTransaction)
                    .await?
                {
                    return Ok(revision);
                }
            }
        }
        let revisions = self
            .db
            .sku_revisions()
            .find_many(doc! { "sku_id": sku_id }, &mut NoTransaction)
            .await?;
        revisions
            .into_iter()
            .max_by_key(|revision| revision.revision.revision_no)
            .ok_or_else(|| Error::NotFound("SKU 修订不存在".to_string()))
    }

    /// 计算某卡券类目 SKU 已有扩展修订的最大序号 + 1。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn next_voucher_profile_revision_no(&self, sku_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .voucher_category_profile_revisions()
            .find_many(doc! { "sku_id": sku_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }
}

// ---------- 自由函数辅助 ----------

/// 校验卡券类目创建请求的分类选择：`category_id` 与 `new_category` 不可同时给出；
/// 两者都缺省时由服务端解析共用卡券根分类。
///
/// # 参数
/// * `category_id` - 引用已有分类
/// * `new_category` - 内联新建分类
///
/// # 返回
/// 合法时返回 `Ok(())`。
///
/// # 错误
/// 两者同时给出时返回 `ValidationError`。
fn ensure_category_selection_exclusive(
    category_id: &Option<ProductCategoryId>,
    new_category: &Option<NewVoucherCategoryInput>,
) -> Result<()> {
    match (category_id, new_category) {
        (Some(_), Some(_)) => Err(Error::ValidationError(
            "分类只能二选一：引用已有分类或新建分类".to_string(),
        )),
        (Some(_), None) | (None, Some(_)) | (None, None) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_category_input() -> NewVoucherCategoryInput {
        NewVoucherCategoryInput {
            category_code: "VC-CAT".to_string(),
            parent_category_id: None,
            name: "卡券分类".to_string(),
        }
    }

    #[test]
    fn ensure_category_selection_exclusive_accepts_exactly_one() {
        assert!(ensure_category_selection_exclusive(&Some(ProductCategoryId::new("cat-1")), &None).is_ok());
        assert!(ensure_category_selection_exclusive(&None, &Some(new_category_input())).is_ok());
    }

    #[test]
    fn ensure_category_selection_exclusive_rejects_both_given() {
        assert!(ensure_category_selection_exclusive(
            &Some(ProductCategoryId::new("cat-1")),
            &Some(new_category_input())
        )
        .is_err());
    }

    #[test]
    fn ensure_category_selection_exclusive_allows_neither_for_default_root() {
        assert!(
            ensure_category_selection_exclusive(&None, &None).is_ok(),
            "两者都缺省时走共用卡券根分类"
        );
    }
}
