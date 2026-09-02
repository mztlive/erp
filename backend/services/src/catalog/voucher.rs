use database::{AccessControlExt, CatalogExt, NoTransaction, Transactional};
use entities::catalog::product::{Product, ProductData};
use entities::catalog::product_category::{ProductCategory, ProductCategoryData};
use entities::catalog::product_revision::{ProductRevision, ProductRevisionData};
use entities::catalog::sku::Sku;
use entities::catalog::sku_revision::SkuRevision;
use entities::catalog::voucher_category_profile_revision::VoucherCategoryProfileRevision;
use entities::catalog::{
    next_revision_no, EnableStatus, ProductCategoryId, ProductId, ProductKind, ProductRevisionId, SkuId,
    SkuRevisionId, VoucherCategoryProfileRevisionId, VoucherCategorySelection,
};
use entities::common::time::BusinessDate;
use id_generator::next_id;
use validator::Validate;

use super::sku_edit::{NewSkuContext, SkuEditItem};
use super::CatalogService;
use crate::audit::AuditActor;
use crate::catalog::dto::{
    CreateVoucherCategoryRequest, NewVoucherCategoryInput, PageView, SortDir, UpdateVoucherCategoryRequest,
    VoucherCategoryProfileListParams, VoucherCategoryProfileView, VoucherSkuInput,
};
use crate::errors::{Error, Result};

/// 卡券类目扩展修订仓储筛选条件类型。
type VoucherCategoryProfileRevisionFilter =
    <mongodb::Database as CatalogExt>::VoucherCategoryProfileRevisionFilter;

/// 卡券类目原子创建草稿。
struct VoucherCategoryDraft {
    /// 内联新建的分类；引用已有或默认根分类时为空。
    new_category: Option<ProductCategory>,
    /// SPU 稳定身份。
    product: Product,
    /// 首个商品修订。
    revision: ProductRevision,
    /// 唯一 SKU 与首个修订。
    sku_item: SkuEditItem,
    /// 首个卡券类目扩展修订。
    voucher_revision: VoucherCategoryProfileRevision,
}

/// 卡券类目更新草稿。
struct VoucherCategoryUpdateDraft {
    /// 已更新当前修订指针的商品。
    product: Product,
    /// 新商品修订。
    product_revision: ProductRevision,
    /// 已更新当前修订指针的 SKU。
    sku: Sku,
    /// 新 SKU 修订。
    sku_revision: SkuRevision,
    /// 新卡券类目扩展修订。
    voucher_revision: VoucherCategoryProfileRevision,
}

/// 已解析字典与缺省值的卡券类目创建输入。
struct ResolvedVoucherCategoryInput {
    voucher_no: String,
    name: String,
    description: String,
    specification: Option<String>,
    category_id: ProductCategoryId,
    new_category: Option<ProductCategory>,
    brand_id: entities::ids::ProductBrandId,
    sku: VoucherSkuInput,
    status: Option<EnableStatus>,
    effective_from: Option<BusinessDate>,
    effective_to: Option<BusinessDate>,
}

impl CatalogService {
    /// 分页查询卡券类目扩展修订列表。
    ///
    /// # 参数
    /// * `params` - SKU、状态、分页与排序筛选参数
    ///
    /// # 返回
    /// 返回已批量装配 SKU 编号、商品版本和当前名称的分页视图。
    ///
    /// # 错误
    /// 分页或排序参数非法，以及仓储查询失败时返回错误。
    pub async fn voucher_category_profile_list(
        &self,
        params: &VoucherCategoryProfileListParams,
    ) -> Result<PageView<VoucherCategoryProfileView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = VoucherCategoryProfileRevisionFilter {
            sku_id: query.sku_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .catalog()
            .voucher_profile_page(&filter, &mut NoTransaction)
            .await?;
        Ok(PageView {
            items: page
                .items
                .into_iter()
                .map(|row| VoucherCategoryProfileView {
                    id: row.id,
                    sku_id: row.sku_id,
                    sku_no: row.sku_no,
                    product_id: row.product_id,
                    product_version: row.product_version,
                    name: row.name,
                    revision_no: row.revision_no,
                    description: row.description,
                    status: row.status,
                    created_at: row.created_at,
                    version: row.version,
                })
                .collect(),
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 更新卡券类目名称与描述。
    ///
    /// 按 SKU 稳定身份定位所属卡券商品，追加商品、SKU 与扩展修订，并在同一事务
    /// 更新两个稳定主表的当前修订指针。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU 稳定 ID
    /// * `req` - 名称、描述、生效区间与商品期望版本
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新扩展修订及其关联展示上下文。
    ///
    /// # 错误
    /// 请求非法、关系缺失、目标不是卡券商品、版本冲突或事务失败时返回错误。
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
        self.voucher_profile_view(&revision).await
    }

    /// 原子创建卡券类目。
    ///
    /// 一个卡券类目对应一个 `VOUCHER` 商品和唯一 SKU；分类、品牌与基础单位缺省时
    /// 分别使用共用卡券根分类、“福尚云”和“张”。
    ///
    /// # 参数
    /// * `req` - 卡券编号、名称、描述、字典引用与唯一 SKU 输入
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回首个卡券类目扩展修订及其关联展示上下文。
    ///
    /// # 错误
    /// 请求、字典、条码或分类关系非法，以及唯一约束或事务失败时返回错误。
    pub async fn voucher_category_create(
        &self,
        req: CreateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileView> {
        req.validate()?;
        let draft = self.build_voucher_category_draft(req, actor).await?;
        let revision = self.write_voucher_category_draft(draft, actor).await?;
        self.voucher_profile_view(&revision).await
    }

    /// 装配单个卡券类目扩展修订的响应视图。
    ///
    /// # 参数
    /// * `revision` - 已写入的卡券类目扩展修订
    ///
    /// # 返回
    /// 返回已补齐 SKU、商品和当前名称关系的响应视图。
    ///
    /// # 错误
    /// 仓储关系查询失败时返回错误。
    async fn voucher_profile_view(
        &self,
        revision: &VoucherCategoryProfileRevision,
    ) -> Result<VoucherCategoryProfileView> {
        let row = self
            .db
            .catalog()
            .voucher_profile(revision, &mut NoTransaction)
            .await?;
        Ok(VoucherCategoryProfileView {
            id: row.id,
            sku_id: row.sku_id,
            sku_no: row.sku_no,
            product_id: row.product_id,
            product_version: row.product_version,
            name: row.name,
            revision_no: row.revision_no,
            description: row.description,
            status: row.status,
            created_at: row.created_at,
            version: row.version,
        })
    }

    /// 构造卡券类目原子创建草稿。
    ///
    /// # 参数
    /// * `req` - 已通过 DTO 校验的创建请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回全部 ID 已生成且领域实体已校验的待写入草稿。
    ///
    /// # 错误
    /// 分类选择、字典关系、条码或实体字段违反规则时返回错误。
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
        let (category_id, new_category) = self
            .resolve_voucher_category(category_id, new_category, actor)
            .await?;
        let brand_id = match brand_id {
            Some(brand_id) => {
                self.load_brand(brand_id.as_ref()).await?;
                brand_id
            }
            None => self.ensure_voucher_default_brand(actor).await?,
        };
        let sku = match sku {
            Some(sku) => sku,
            None => VoucherSkuInput::default_for_unit(self.ensure_voucher_default_unit(actor).await?),
        };
        self.ensure_brand_and_unit_ok(&brand_id, std::iter::once(&sku.base_unit_id))
            .await?;
        self.assemble_voucher_category_draft(
            ResolvedVoucherCategoryInput {
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
            },
            actor,
        )
        .await
    }

    /// 解析卡券类目创建请求的分类来源。
    ///
    /// # 参数
    /// * `category_id` - 可选已有分类
    /// * `new_category` - 可选内联新分类
    /// * `actor` - 新分类创建人
    ///
    /// # 返回
    /// 返回最终分类 ID 与可选待写入新分类实体。
    ///
    /// # 错误
    /// 两种来源同时给出、已有分类不是卡券类型或父链非法时返回错误。
    async fn resolve_voucher_category(
        &self,
        category_id: Option<ProductCategoryId>,
        new_category: Option<NewVoucherCategoryInput>,
        actor: &AuditActor,
    ) -> Result<(ProductCategoryId, Option<ProductCategory>)> {
        let selection = VoucherCategorySelection::from_options(category_id, new_category)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        match selection {
            VoucherCategorySelection::Existing(category_id) => {
                let category = self.load_category(category_id.as_ref()).await?;
                if category.product_kind != ProductKind::Voucher {
                    return Err(Error::BusinessLogicError(
                        "所选分类不允许 VOUCHER 类型".to_string(),
                    ));
                }
                Ok((category_id, None))
            }
            VoucherCategorySelection::New(input) => {
                let category_id = ProductCategoryId::new(next_id());
                self.ensure_parent_chain_ok(category_id.as_ref(), input.parent_category_id.as_ref())
                    .await?;
                let category = ProductCategory::new(
                    category_id.clone(),
                    ProductCategoryData {
                        category_code: input.category_code,
                        parent_category_id: input.parent_category_id,
                        name: input.name,
                        product_kind: ProductKind::Voucher,
                        status: EnableStatus::Active,
                    },
                    actor.id(),
                )?;
                Ok((category_id, Some(category)))
            }
            VoucherCategorySelection::DefaultRoot => {
                Ok((self.ensure_voucher_root_category(actor).await?, None))
            }
        }
    }

    /// 组装卡券类目创建所需的商品、修订、唯一 SKU 与扩展修订。
    ///
    /// # 参数
    /// * `input` - 已解析字典、缺省值与唯一 SKU 的创建输入
    /// * `actor` - 创建人
    ///
    /// # 返回
    /// 返回全部实体已通过领域校验的创建草稿。
    ///
    /// # 错误
    /// 条码占用或任一实体不变式校验失败时返回错误。
    async fn assemble_voucher_category_draft(
        &self,
        input: ResolvedVoucherCategoryInput,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryDraft> {
        let ResolvedVoucherCategoryInput {
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
        } = input;
        let effective_from = effective_from.unwrap_or_else(BusinessDate::today);
        let product_status = status.unwrap_or(EnableStatus::Active);
        let product_id = ProductId::new(next_id());
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
                    effective_from,
                    effective_to,
                    created_by: actor.id(),
                },
                sku.into_product_sku(voucher_no, name.clone()),
            )
            .await?;
        let revision = ProductRevision::new(
            ProductRevisionId::new(next_id()),
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
        product.attach_revision(&revision, actor.id())?;
        let voucher_revision = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new(next_id()),
            entities::catalog::voucher_category_profile_revision::VoucherCategoryProfileRevisionData {
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

    /// 在单个事务内写入卡券类目创建草稿。
    ///
    /// # 参数
    /// * `draft` - 商品、修订、唯一 SKU、扩展修订和可选新分类草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回写入后的卡券类目扩展修订。
    ///
    /// # 错误
    /// 唯一约束、事务写入或审计写入失败时整体回滚并返回错误。
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

    /// 构造卡券类目更新草稿。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU 稳定 ID
    /// * `req` - 名称、描述、生效区间与商品期望版本
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回已追加三个后继修订并更新稳定指针的事务草稿。
    ///
    /// # 错误
    /// SKU、商品或当前修订缺失，目标不是卡券商品，版本冲突或实体校验失败时返回错误。
    async fn build_voucher_category_update_draft(
        &self,
        sku_id: &str,
        req: UpdateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryUpdateDraft> {
        let mut sku = self
            .db
            .catalog()
            .sku(sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡券类目 SKU 不存在".to_string()))?;
        let mut product = self.load_product(sku.product_id.as_ref()).await?;
        if product.product_kind != ProductKind::Voucher {
            return Err(Error::BusinessLogicError(
                "目标 SKU 不属于卡券类目商品".to_string(),
            ));
        }
        ensure_product_version(&product, req.version)?;
        let current_product_revision = self
            .db
            .catalog()
            .current_product_revision(&product, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品修订不存在".to_string()))?;
        let current_sku_revision = self
            .db
            .catalog()
            .current_sku_revision(&sku, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("SKU 修订不存在".to_string()))?;
        let sku_id = SkuId::new(sku_id);
        let current_voucher_revision = self
            .db
            .catalog()
            .current_voucher_profile_revision(&sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡券类目扩展修订不存在".to_string()))?;
        let effective_from = req.effective_from.unwrap_or_else(BusinessDate::today);
        let product_revision = current_product_revision.content_successor(
            ProductRevisionId::new(next_id()),
            self.next_product_revision_no(&ProductId::new(product.base.id.clone()))
                .await?,
            req.name.clone(),
            Some(req.description.clone()),
            effective_from,
            req.effective_to,
        )?;
        product.attach_revision(&product_revision, actor.id())?;
        let sku_revision = current_sku_revision.content_successor(
            SkuRevisionId::new(next_id()),
            self.next_sku_revision_no(&sku_id).await?,
            req.name,
            Some(req.description.clone()),
            effective_from,
            req.effective_to,
        )?;
        sku.attach_revision(&sku_revision, actor.id())?;
        let latest_voucher_revision_no = self
            .db
            .catalog()
            .latest_voucher_profile_revision_no(&sku_id, &mut NoTransaction)
            .await?;
        let voucher_revision = current_voucher_revision.content_successor(
            VoucherCategoryProfileRevisionId::new(next_id()),
            next_revision_no(latest_voucher_revision_no)?,
            req.description,
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
    /// * `draft` - 已更新稳定指针并生成后继修订的草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回写入后的卡券类目扩展修订。
    ///
    /// # 错误
    /// 乐观锁、唯一约束、事务或审计写入失败时整体回滚并返回错误。
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
}

/// 校验商品乐观锁版本。
///
/// # 参数
/// * `product` - 当前商品稳定实体
/// * `expected` - 客户端读取时看到的期望版本
///
/// # 返回
/// 当前版本与期望版本一致时返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回稳定的 409 冲突错误。
fn ensure_product_version(product: &Product, expected: u64) -> Result<()> {
    if !product.has_version(expected) {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}
