/** 商品列表查询、筛选选项、SKU 摘要与整组上/下架适配。 */

import { apiPut } from "@/lib/api"
import { hasPermission } from "@/lib/permissions"
import type {
    ProductBrandDto,
    ProductCategoryDto,
    ProductDto,
    ProductListingDto,
    SkuDto,
    SkuRevisionDto,
    SupplierDto,
    UnitOfMeasureDto,
} from "@/features/master-data/api/contracts"
import { mapProductRow } from "@/features/master-data/api/list-mappers"
import { asLifecycle } from "@/features/master-data/api/presentation"
import type {
    MasterDataListItem,
    MasterDataListQuery,
    ProductFilterOptions,
    ProductListSkuSummary,
    ProductListingStatus,
} from "@/features/master-data/types"
import { fetchCompleteList } from "@/lib/collect-pages"
import { fetchAllPages } from "./fetch-all"

/** 读取 fetchAllPages 挂上的响应字段；没有该字段时返回 undefined，不编造。 */
function attachedEmptyReason(value: object): string | null | undefined {
    if (!("empty_reason" in value)) return undefined
    const reason = (value as { empty_reason?: unknown }).empty_reason
    if (typeof reason === "string" || reason === null) return reason
    return undefined
}

export async function listProducts(query: MasterDataListQuery): Promise<{
    rows: MasterDataListItem[]
    emptyReason?: string | null
}> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const page = await fetchCompleteList<ProductDto>("/admin/products", {
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
        owner_user_ids: query.ownerUserIds || undefined,
        procurement_owner_user_ids: query.procurementOwnerUserIds || undefined,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants: query.includeDescendants || undefined,
    })
    return {
        rows: page.items.map((product) => mapProductRow(product)),
        emptyReason: page.empty_reason,
    }
}

/** 读取商品筛选使用的授权分类、品牌与供应商选项（含停用）。 */
export async function fetchProductFilterOptions(
    permissions: readonly string[],
): Promise<ProductFilterOptions> {
    const [categories, brands, suppliers] = await Promise.all([
        hasPermission(permissions, "product_category:list")
            ? fetchAllPages<ProductCategoryDto>("/admin/product-categories", {
                  sort_by: "name",
                  sort_dir: "asc",
              })
            : [],
        hasPermission(permissions, "product_brand:list")
            ? fetchAllPages<ProductBrandDto>("/admin/product-brands", {
                  sort_by: "name",
                  sort_dir: "asc",
              })
            : [],
        hasPermission(permissions, "supplier:list")
            ? fetchAllPages<SupplierDto>("/admin/suppliers", {})
            : [],
    ])
    const supplierOptions = suppliers
        .map((supplier) => ({
            value: supplier.id,
            label:
                (supplier.short_name ??
                    supplier.legal_name ??
                    supplier.supplier_no) +
                (supplier.status === "active" ? "" : "（停用）"),
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
    const unavailable: ("categories" | "brands" | "suppliers")[] = []
    if (!hasPermission(permissions, "product_category:list"))
        unavailable.push("categories")
    if (!hasPermission(permissions, "product_brand:list"))
        unavailable.push("brands")
    if (!hasPermission(permissions, "supplier:list"))
        unavailable.push("suppliers")
    const emptyReasons: {
        categories?: string | null
        brands?: string | null
        suppliers?: string | null
    } = {}
    const categoryEmptyReason = attachedEmptyReason(categories)
    const brandEmptyReason = attachedEmptyReason(brands)
    const supplierEmptyReason = attachedEmptyReason(suppliers)
    if (categoryEmptyReason !== undefined)
        emptyReasons.categories = categoryEmptyReason
    if (brandEmptyReason !== undefined) emptyReasons.brands = brandEmptyReason
    if (supplierEmptyReason !== undefined)
        emptyReasons.suppliers = supplierEmptyReason
    return {
        unavailable,
        categories: categories.map((category) => ({
            categoryId: category.id,
            categoryCode: category.category_code,
            categoryName:
                category.name +
                (category.status === "active" ? "" : "（停用）"),
            parentId: category.parent_category_id ?? undefined,
        })),
        brands: brands.map((brand) => ({
            value: brand.id,
            label: brand.name + (brand.status === "active" ? "" : "（停用）"),
            keywords: `${brand.brand_code} ${brand.name}`,
        })),
        suppliers: supplierOptions,
        ...(Object.keys(emptyReasons).length > 0 ? { emptyReasons } : {}),
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
