import type {
    CommercialProfileDto,
    ProductBrandDto,
    ProductCategoryDto,
    ProductDto,
    ProductRevisionDto,
    SellableSkuDto,
    SkuDto,
    SupplierDto,
    UnitOfMeasureDto,
    VoucherCategoryProfileDto,
    WarehouseDto,
    WarehouseRevisionDto,
} from "@/features/master-data/api/contracts"
import {
    asLifecycle,
    commonActions,
    invoiceLabel,
    lifecycleLabel,
    lifecycleTone,
    productKindLabel,
    settlementLabel,
    todayDateOnly,
    tsToIso,
} from "@/features/master-data/api/presentation"
import type {
    MasterDataListItem,
    ProductListingStatus,
} from "@/features/master-data/types"

export function isFutureDate(date: string | undefined): boolean {
    if (!date) return false
    return date > todayDateOnly()
}

export function mapCategoryRow(dto: ProductCategoryDto): MasterDataListItem {
    const lifecycle = asLifecycle(dto.status)
    return {
        objectType: "categories",
        stableId: dto.id,
        stableNo: dto.category_code,
        name: dto.name,
        dictionaryCode: dto.category_code,
        parentStableId: dto.parent_category_id ?? undefined,
        productKind: productKindLabel(dto.product_kind),
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: dto.id,
        displayedRevisionId: dto.id,
        revisionNo: dto.version,
        effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
        keyFacts: [
            { label: "分类代码", value: dto.category_code },
            {
                label: "上级分类",
                value: dto.parent_category_id
                    ? dto.parent_category_id
                    : "（根分类）",
            },
            {
                label: "适用商品类型",
                value: productKindLabel(dto.product_kind),
            },
        ],
        selectorEligibility: [],
        ...commonActions("categories", lifecycle),
        lockVersion: dto.version,
        metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
    }
}

export function mapBrandRow(dto: ProductBrandDto): MasterDataListItem {
    const lifecycle = asLifecycle(dto.status)
    return {
        objectType: "brands",
        stableId: dto.id,
        stableNo: dto.brand_code,
        name: dto.name,
        dictionaryCode: dto.brand_code,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: dto.id,
        displayedRevisionId: dto.id,
        revisionNo: dto.version,
        effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
        keyFacts: [{ label: "品牌代码", value: dto.brand_code }],
        selectorEligibility: [],
        ...commonActions("brands", lifecycle),
        lockVersion: dto.version,
        metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
    }
}

export function mapUnitOfMeasureRow(dto: UnitOfMeasureDto): MasterDataListItem {
    const lifecycle = asLifecycle(dto.status)
    return {
        objectType: "unit-of-measures",
        stableId: dto.id,
        stableNo: dto.unit_code,
        name: dto.name,
        dictionaryCode: dto.unit_code,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: dto.id,
        displayedRevisionId: dto.id,
        revisionNo: dto.version,
        effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
        keyFacts: [
            { label: "单位代码", value: dto.unit_code },
            { label: "单位符号", value: dto.symbol },
            { label: "数量小数位", value: String(dto.quantity_scale) },
        ],
        selectorEligibility: [],
        ...commonActions("unit-of-measures", lifecycle),
        lockVersion: dto.version,
        metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
    }
}

export function mapProductRow(
    dto: ProductDto,
    revision?: ProductRevisionDto,
): MasterDataListItem {
    const lifecycle = asLifecycle(dto.status)
    const listingStatus =
        dto.listing_status.toUpperCase() as ProductListingStatus
    const future = revision ? isFutureDate(revision.effective_from) : false
    const productName = revision?.name ?? dto.name ?? dto.product_no
    return {
        objectType: "products",
        stableId: dto.id,
        stableNo: dto.product_no,
        name: productName,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        listingStatus,
        listedSkuCount: dto.listed_sku_count,
        skuCount: dto.sku_count,
        revisionTiming: future ? "FUTURE" : "CURRENT",
        revisionTimingLabel: future ? "待生效" : "当前生效",
        currentRevisionId: revision?.id ?? dto.id,
        displayedRevisionId: revision?.id ?? dto.id,
        revisionNo: revision?.revision_no ?? dto.version,
        effectiveFrom:
            revision?.effective_from ?? tsToIso(dto.created_at).slice(0, 10),
        keyFacts: [
            { label: "商品编号", value: dto.product_no },
            { label: "商品类型", value: productKindLabel(dto.product_kind) },
            {
                label: "上架 SKU",
                value: `${dto.listed_sku_count}/${dto.sku_count}`,
            },
            {
                label: "有供给 SKU",
                value: `${dto.supplied_sku_count ?? 0}/${dto.sku_count}`,
            },
            {
                label: "已填销售价 SKU",
                value: `${dto.priced_sku_count ?? 0}/${dto.sku_count}`,
            },
        ],
        primaryBlocker:
            lifecycle === "DISABLED" ? "已停用：历史引用保留" : undefined,
        selectorEligibility: [],
        ...commonActions("products", lifecycle),
        lockVersion: dto.version,
        metricTags: [
            lifecycle === "ENABLED" ? "enabled" : "disabled",
            ...(future ? (["pending"] as const) : []),
        ],
        // 稳定码（PHYSICAL/VOUCHER…），展示文案仍在 keyFacts「商品类型」
        productKind: dto.product_kind,
    }
}

export function mapSkuAsSellable(dto: SellableSkuDto): MasterDataListItem {
    const lifecycle = "ENABLED" as const
    const kindLabel = productKindLabel(dto.product_kind)
    const specificationAttributes = dto.specification_attributes ?? []
    const specificationLabel =
        specificationAttributes.length > 0
            ? specificationAttributes
                  .map((attribute) => `${attribute.name}：${attribute.value}`)
                  .join(" / ")
            : "无规格"
    const baseUnit = dto.base_unit_name ?? dto.base_unit_code ?? "—"
    return {
        objectType: "sellable-items",
        stableId: dto.sku_id,
        stableNo: dto.sku_no,
        name: dto.name,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: dto.sku_revision_id,
        displayedRevisionId: dto.sku_revision_id,
        revisionNo: dto.sku_revision_no,
        effectiveFrom: dto.effective_from,
        effectiveTo: dto.effective_to ?? undefined,
        keyFacts: [
            { label: "SKU", value: dto.sku_no },
            {
                label: "销售价",
                value: `¥${dto.sales_visible_price_gross}`,
            },
            {
                label: "商品编号",
                value: dto.product_no,
            },
            ...(dto.base_unit_name || dto.base_unit_code
                ? [
                      {
                          label: "基础单位",
                          value: dto.base_unit_name ?? dto.base_unit_code!,
                      },
                  ]
                : []),
            ...(kindLabel ? [{ label: "商品类型", value: kindLabel }] : []),
            { label: "有效供应商", value: `${dto.supplier_count} 家` },
            ...(dto.supply_regions.length > 0
                ? [{ label: "可供区域", value: dto.supply_regions.join("、") }]
                : []),
        ],
        selectorEligibility: [],
        ...commonActions("sellable-items", lifecycle),
        lockVersion: dto.sku_version,
        metricTags: ["enabled"],
        productKind: dto.product_kind,
        sellableItem: {
            productId: dto.product_id,
            productNo: dto.product_no,
            specificationAttributes,
            specificationLabel,
            barcode: dto.barcode ?? undefined,
            baseUnit,
            productKindLabel: kindLabel,
            salesVisiblePriceGross: dto.sales_visible_price_gross,
            marketPrice: dto.market_price ?? undefined,
            supplierCount: dto.supplier_count,
            supplyRegions: dto.supply_regions,
            eligibilityAsOf: dto.eligibility_as_of,
            mainImageAssetId: dto.main_image_asset_id?.trim() || undefined,
        },
    }
}

export function mapVoucherRow(
    profile: VoucherCategoryProfileDto,
    sku?: SkuDto,
): MasterDataListItem {
    const lifecycle = asLifecycle(profile.status)
    const skuNo = profile.sku_no ?? sku?.sku_no ?? profile.sku_id
    const displayName = profile.name?.trim() || profile.description
    // 稳定身份 = SKU（创建后不变）；列表按 SKU 聚合最新扩展修订。
    return {
        objectType: "voucher-categories",
        stableId: profile.sku_id,
        stableNo: skuNo,
        name: displayName,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: profile.id,
        displayedRevisionId: profile.id,
        revisionNo: profile.revision_no,
        effectiveFrom: tsToIso(profile.created_at).slice(0, 10),
        keyFacts: [
            { label: "卡券 SKU", value: skuNo },
            { label: "说明", value: profile.description },
        ],
        primaryBlocker: lifecycle === "DISABLED" ? "已停用" : undefined,
        selectorEligibility: [],
        ...commonActions("voucher-categories", lifecycle),
        lockVersion: profile.product_version ?? profile.version,
        metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
    }
}

export function mapWarehouseRow(
    wh: WarehouseDto,
    revision?: WarehouseRevisionDto,
): MasterDataListItem {
    const lifecycle = asLifecycle(wh.status)
    return {
        objectType: "warehouses",
        stableId: wh.id,
        stableNo: wh.warehouse_code,
        name: revision?.name ?? wh.warehouse_code,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: revision?.id ?? wh.id,
        displayedRevisionId: revision?.id ?? wh.id,
        revisionNo: revision?.revision_no ?? wh.version,
        effectiveFrom:
            revision?.effective_from ?? tsToIso(wh.created_at).slice(0, 10),
        effectiveTo: revision?.effective_to ?? undefined,
        keyFacts: [
            { label: "仓库代码", value: wh.warehouse_code },
            ...(revision
                ? [{ label: "变更原因", value: revision.change_reason }]
                : []),
        ],
        primaryBlocker: "暂不可维护（本期）",
        selectorEligibility: [],
        ...commonActions("warehouses", lifecycle),
        lockVersion: wh.version,
        metricTags: [
            lifecycle === "ENABLED" ? "enabled" : "disabled",
            "pending",
        ],
    }
}

export function mapSupplierRow(
    supplier: SupplierDto,
    partyName = supplier.legal_name ?? supplier.short_name ?? undefined,
    profile: CommercialProfileDto | null = supplier.current_profile,
): MasterDataListItem {
    const lifecycle = asLifecycle(supplier.status)
    return {
        objectType: "suppliers",
        stableId: supplier.id,
        stableNo: supplier.supplier_no,
        name: partyName || supplier.supplier_no,
        lifecycleStatus: lifecycle,
        lifecycleStatusLabel: lifecycleLabel(lifecycle),
        lifecycleTone: lifecycleTone(lifecycle),
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId:
            supplier.current_commercial_profile_revision_id ?? supplier.id,
        displayedRevisionId:
            supplier.current_commercial_profile_revision_id ?? supplier.id,
        revisionNo: profile?.revision_no ?? supplier.version,
        effectiveFrom: tsToIso(
            profile?.created_at ?? supplier.created_at,
        ).slice(0, 10),
        keyFacts: [
            {
                label: "结算方式",
                value: settlementLabel(profile?.settlement_mode) || "—",
            },
            {
                label: "发票类型",
                value: invoiceLabel(profile?.invoice_type) || "—",
            },
        ],
        primaryBlocker: lifecycle === "DISABLED" ? "已停用" : undefined,
        selectorEligibility: [],
        ...commonActions("suppliers", lifecycle),
        lockVersion: supplier.version,
        metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
    }
}
