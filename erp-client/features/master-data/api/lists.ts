import { apiGet, apiPut } from "@/lib/api"
import type {
    BackendPage,
    ProductBrandDto,
    ProductCategoryDto,
    ProductDto,
    ProductListingDto,
    SellableSkuDto,
    SkuDto,
    SkuRevisionDto,
    SupplierDto,
    UnitOfMeasureDto,
    VoucherCategoryProfileDto,
    WarehouseDto,
    WarehouseRevisionDto,
} from "@/features/master-data/api/contracts"
import {
    asLifecycle,
    LIST_PAGE_SIZE,
    productKindLabel,
} from "@/features/master-data/api/presentation"
import {
    mapBrandRow,
    mapCategoryRow,
    mapProductRow,
    mapSkuAsSellable,
    mapSupplierRow,
    mapUnitOfMeasureRow,
    mapVoucherRow,
    mapWarehouseRow,
} from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListItem,
    MasterDataListQuery,
    ProductFilterOptions,
    ProductListSkuSummary,
    ProductListingStatus,
} from "@/features/master-data/types"

export async function fetchAllPages<T>(
    path: string,
    query: Record<string, unknown> = {},
): Promise<T[]> {
    const items: T[] = []
    let page = 1
    let total = Number.POSITIVE_INFINITY
    while (items.length < total) {
        const result = await apiGet<BackendPage<T>>(path, {
            ...query,
            page,
            page_size: LIST_PAGE_SIZE,
        })
        items.push(...result.items)
        total = result.total
        if (result.items.length === 0) break
        page += 1
        if (page > 50) break
    }
    return items
}

export async function listCategories(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const items = await fetchAllPages<ProductCategoryDto>(
        "/admin/product-categories",
        {
            status,
            name: query.q || undefined,
        },
    )
    // Resolve parent names for keyFacts
    const byId = new Map(items.map((c) => [c.id, c]))
    return items.map((dto) => {
        const row = mapCategoryRow(dto)
        if (dto.parent_category_id) {
            const parent = byId.get(dto.parent_category_id)
            return {
                ...row,
                keyFacts: [
                    { label: "分类代码", value: dto.category_code },
                    {
                        label: "上级分类",
                        value: parent?.name ?? "（未知上级）",
                    },
                    {
                        label: "适用商品类型",
                        value: productKindLabel(dto.product_kind),
                    },
                ],
            }
        }
        return row
    })
}

export async function listBrands(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const items = await fetchAllPages<ProductBrandDto>(
        "/admin/product-brands",
        {
            status,
            name: query.q || undefined,
        },
    )
    return items.map(mapBrandRow)
}

export async function listUnitOfMeasures(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    // 仅按 status 拉全量（字典体量小）；代码/名称/符号在本地模糊匹配
    const items = await fetchAllPages<UnitOfMeasureDto>(
        "/admin/unit-of-measures",
        { status },
    )
    const rows = items.map(mapUnitOfMeasureRow)
    const q = query.q?.trim().toLowerCase()
    if (!q) return rows
    return rows.filter((row) => {
        const hay = [
            row.stableNo,
            row.name,
            ...row.keyFacts.map((f) => f.value),
        ]
            .join(" ")
            .toLowerCase()
        return hay.includes(q)
    })
}

export async function listProducts(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const products = await fetchAllPages<ProductDto>("/admin/products", {
        status,
        keyword: query.q || undefined,
        product_kind: query.productKind,
        category_id: query.productCategoryId,
        brand_id: query.productBrandId,
        supplier_id: query.productSupplierId,
        listing_status: query.productListingStatus,
        supply_coverage: query.productSupplyCoverage,
        sales_price_min: query.productSalesPriceMin,
        sales_price_max: query.productSalesPriceMax,
    })
    return products.map((product) => mapProductRow(product))
}

/** 读取商品筛选使用的启用分类、品牌与供应商选项。 */
export async function fetchProductFilterOptions(): Promise<ProductFilterOptions> {
    const [categories, brands, suppliers] = await Promise.all([
        fetchAllPages<ProductCategoryDto>("/admin/product-categories", {
            status: "active",
            sort_by: "name",
            sort_dir: "asc",
        }),
        fetchAllPages<ProductBrandDto>("/admin/product-brands", {
            status: "active",
            sort_by: "name",
            sort_dir: "asc",
        }),
        fetchAllPages<SupplierDto>("/admin/suppliers", {
            status: "active",
        }),
    ])
    const supplierOptions = suppliers
        .map((supplier) => ({
            value: supplier.id,
            label:
                supplier.short_name ??
                supplier.legal_name ??
                supplier.supplier_no,
            keywords: [
                supplier.supplier_no,
                supplier.party_no,
                supplier.short_name,
                supplier.legal_name,
            ]
                .filter(Boolean)
                .join(" "),
        }))
        .sort((left, right) => left.label.localeCompare(right.label, "zh-CN"))
    return {
        categories: categories.map((category) => ({
            categoryId: category.id,
            categoryCode: category.category_code,
            categoryName: category.name,
            parentId: category.parent_category_id ?? undefined,
        })),
        brands: brands.map((brand) => ({
            value: brand.id,
            label: brand.name,
            keywords: `${brand.brand_code} ${brand.name}`,
        })),
        suppliers: supplierOptions,
    }
}

/**
 * 读取商品列表当前页的启用 SKU 与当前销售价。
 *
 * 商品列表接口只返回 SKU 数量；这里按稳定商品 ID 补齐 SKU 当前修订，供列表展示
 * 销售价范围，并为新增供给 Dialog 提供固定 SKU 身份。
 */
export async function fetchProductListSkus(
    productIds: readonly string[],
): Promise<readonly ProductListSkuSummary[]> {
    const selectedProductIds = new Set(productIds.filter(Boolean))
    if (selectedProductIds.size === 0) return []

    const [skus, units] = await Promise.all([
        fetchAllPages<SkuDto>("/admin/skus", {}),
        fetchAllPages<UnitOfMeasureDto>("/admin/unit-of-measures", {}),
    ])
    const unitById = new Map(units.map((unit) => [unit.id, unit]))
    const selectedSkus = skus.filter(
        (sku) =>
            selectedProductIds.has(sku.product_id) &&
            asLifecycle(sku.status) === "ENABLED",
    )

    return Promise.all(
        selectedSkus.map(async (sku) => {
            const revisions = await fetchAllPages<SkuRevisionDto>(
                "/admin/sku-revisions",
                {
                    sku_id: sku.id,
                    sort_by: "revision_no",
                    sort_dir: "desc",
                },
            )
            const revision = sku.current_revision_id
                ? revisions.find((item) => item.id === sku.current_revision_id)
                : undefined
            const unit = unitById.get(sku.base_unit_id)
            return {
                productId: sku.product_id,
                skuId: sku.id,
                skuNo: sku.sku_no,
                skuName: revision?.name ?? sku.sku_no,
                specification:
                    revision?.specification ??
                    sku.specification_signature ??
                    "默认规格",
                baseUnit: unit?.name ?? unit?.symbol ?? unit?.unit_code ?? "—",
                salesVisiblePriceGross:
                    revision?.sales_visible_price_gross ?? undefined,
            }
        }),
    )
}

export async function listSellableItems(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    // 公司商品池是资格投影，仅含当前可销售 SKU；启停/上架/供给覆盖不适用。
    const rows = await fetchAllPages<SellableSkuDto>("/admin/sellable-skus", {
        q: query.q || undefined,
        product_kind: query.productKind,
        category_id: query.productCategoryId,
        brand_id: query.productBrandId,
        supplier_id: query.productSupplierId,
        supply_region: query.supplyRegion,
        sales_price_min: query.productSalesPriceMin,
        sales_price_max: query.productSalesPriceMax,
        eligibility_as_of: query.eligibilityAsOf,
    })
    return rows.map(mapSkuAsSellable)
}

/** 整组切换 SPU 下全部当前启用 SKU 的上架状态。 */
export async function updateProductListingStatus(
    productId: string,
    listingStatus: Exclude<ProductListingStatus, "PARTIALLY_LISTED">,
): Promise<ProductListingDto> {
    return apiPut<ProductListingDto>(
        `/admin/products/${encodeURIComponent(productId)}/listing-status`,
        {
            listing_status: listingStatus === "LISTED" ? "listed" : "unlisted",
        },
    )
}

export async function listVoucherCategories(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    let profiles = await fetchAllPages<VoucherCategoryProfileDto>(
        "/admin/voucher-category-profiles",
        { status },
    ).catch(() => [] as VoucherCategoryProfileDto[])
    // 状态筛选空结果时回退全量，再按启停客户端过滤
    if (profiles.length === 0 && status) {
        profiles = await fetchAllPages<VoucherCategoryProfileDto>(
            "/admin/voucher-category-profiles",
            {},
        ).catch(() => [] as VoucherCategoryProfileDto[])
        if (query.lifecycleStatus === "enabled") {
            profiles = profiles.filter(
                (p) => asLifecycle(p.status) === "ENABLED",
            )
        } else if (query.lifecycleStatus === "disabled") {
            profiles = profiles.filter(
                (p) => asLifecycle(p.status) === "DISABLED",
            )
        }
    }
    if (profiles.length === 0) return []
    // 每个 SKU 只保留最新扩展修订，避免更新后列表出现多行。
    const latestBySku = new Map<string, VoucherCategoryProfileDto>()
    for (const profile of profiles) {
        const prev = latestBySku.get(profile.sku_id)
        if (!prev || profile.revision_no > prev.revision_no) {
            latestBySku.set(profile.sku_id, profile)
        }
    }
    const skus = await fetchAllPages<SkuDto>("/admin/skus", {}).catch(
        () => [] as SkuDto[],
    )
    const skuById = new Map(skus.map((s) => [s.id, s]))
    let rows = Array.from(latestBySku.values()).map((p) =>
        mapVoucherRow(p, skuById.get(p.sku_id)),
    )
    if (query.q?.trim()) {
        const q = query.q.trim().toLowerCase()
        rows = rows.filter(
            (r) =>
                r.name.toLowerCase().includes(q) ||
                r.stableNo.toLowerCase().includes(q),
        )
    }
    return rows
}

export async function listWarehouses(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const warehouses = await fetchAllPages<WarehouseDto>("/admin/warehouses", {
        status,
        warehouse_code: query.q || undefined,
    })
    const rows: MasterDataListItem[] = []
    for (const wh of warehouses) {
        let revision: WarehouseRevisionDto | undefined
        try {
            const revPage = await apiGet<BackendPage<WarehouseRevisionDto>>(
                "/admin/warehouse-revisions",
                {
                    warehouse_id: wh.id,
                    page: 1,
                    page_size: 1,
                    sort_by: "revision_no",
                    sort_dir: "desc",
                },
            )
            revision = revPage.items[0]
        } catch {
            // ignore
        }
        rows.push(mapWarehouseRow(wh, revision))
    }
    return rows
}

export async function listSuppliers(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const suppliers = await fetchAllPages<SupplierDto>("/admin/suppliers", {
        status,
        keyword: query.q || undefined,
        capability_codes: joinFilterCodes(query.supplierCapabilityCodes),
        qualification_types: joinFilterCodes(query.supplierQualificationTypes),
        qualification_health: query.supplierQualificationHealth,
    })
    return suppliers.map((supplier) => mapSupplierRow(supplier))
}

/** 规范化多选条件，供后端以逗号分隔的稳定查询参数接收。 */
export function joinFilterCodes(
    values: readonly string[] | undefined,
): string | undefined {
    if (!values?.length) return undefined
    return [...new Set(values.map((value) => value.trim()).filter(Boolean))]
        .sort()
        .join(",")
}
