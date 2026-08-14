/** 商品列表查询、筛选选项、SKU 摘要与整组上/下架适配。 */

import { apiPut } from "@/lib/api"
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
import { fetchAllPages } from "./fetch-all"

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
