/**
 * 商品对象中心：SPU 当前修订 + SKU 行 + 媒体回显，组装可编辑详情。
 * 历史修订禁止回填当前主档；SKU 行必须能对上当前修订，否则抛错拒绝编辑。
 */

import type {
    ProductBrandDto,
    ProductCategoryDto,
    ProductDto,
    ProductRevisionDto,
    SkuDto,
    SkuRevisionDto,
    UnitOfMeasureDto,
} from "@/features/master-data/api/contracts"
import { mapProductRow, isFutureDate } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import {
    fetchFileAsset,
    resolveMediaAssets,
} from "@/features/master-data/api/media-assets"
import { asLifecycle, tsToIso } from "@/features/master-data/api/presentation"
import type {
    MasterDataCenterView,
    ProductSkuFields,
    RevisionTimelineEntry,
} from "@/features/master-data/types"
import { baseCenter } from "./base"

/** 将后端规范化规格签名还原为 W14 可编辑的 SPU 局部规格名/值。 */
export function parseSpecificationSignature(
    signature: string,
): Array<{ attributeCode: string; valueCode: string }> {
    if (!signature) return []
    return signature.split("|").flatMap((entry) => {
        const separator = entry.indexOf("=")
        if (separator <= 0) return []
        const attributeCode = entry.slice(0, separator).trim()
        const valueCode = entry.slice(separator + 1).trim()
        return attributeCode && valueCode ? [{ attributeCode, valueCode }] : []
    })
}

export async function centerProduct(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const products = await fetchAllPages<ProductDto>("/admin/products", {})
    const product = products.find((p) => p.id === stableId)
    if (!product) return null

    const revisions = await fetchAllPages<ProductRevisionDto>(
        "/admin/product-revisions",
        { product_id: stableId, sort_by: "revision_no", sort_dir: "desc" },
    )
    const currentRev = product.current_revision_id
        ? revisions.find(
              (revision) => revision.id === product.current_revision_id,
          )
        : undefined
    if (!currentRev) {
        throw new Error(
            "商品当前修订不存在或已漂移，禁止以历史修订回填编辑表单",
        )
    }
    const skus = await fetchAllPages<SkuDto>("/admin/skus", {
        product_id: stableId,
    })

    // Units / categories / brands for labels
    const units = await fetchAllPages<UnitOfMeasureDto>(
        "/admin/unit-of-measures",
        {},
    )
    const unitById = new Map(units.map((u) => [u.id, u]))
    const categories = await fetchAllPages<ProductCategoryDto>(
        "/admin/product-categories",
        {},
    )
    const brands = await fetchAllPages<ProductBrandDto>(
        "/admin/product-brands",
        {},
    )

    // SPU 媒体与 SKU 主图按 file_asset 引用解析为可访问地址。
    const carouselMedia = (currentRev?.media ?? [])
        .filter((m) => m.media_role === "carousel")
        .sort((a, b) => a.sort_order - b.sort_order)
    const detailMedia = (currentRev?.media ?? [])
        .filter((m) => m.media_role === "detail")
        .sort((a, b) => a.sort_order - b.sort_order)
    const resolvedAssets = await resolveMediaAssets([
        ...carouselMedia.map((m) => m.file_asset_id),
        ...detailMedia.map((m) => m.file_asset_id),
    ])
    const carouselPreviewUrls: Record<string, string> = {}
    const carouselFileAssetIds: Record<string, string> = {}
    const detailPreviewUrls: Record<string, string> = {}
    const detailFileAssetIds: Record<string, string> = {}
    const carouselImages: string[] = []
    const detailImages: string[] = []
    for (const media of carouselMedia) {
        const asset = resolvedAssets.get(media.file_asset_id)
        const name = asset?.file_name ?? media.file_asset_id
        carouselImages.push(name)
        if (asset?.public_url) carouselPreviewUrls[name] = asset.public_url
        carouselFileAssetIds[name] = media.file_asset_id
    }
    for (const media of detailMedia) {
        const asset = resolvedAssets.get(media.file_asset_id)
        const name = asset?.file_name ?? media.file_asset_id
        detailImages.push(name)
        if (asset?.public_url) detailPreviewUrls[name] = asset.public_url
        detailFileAssetIds[name] = media.file_asset_id
    }

    const skuFields: ProductSkuFields[] = []
    const parsedSpecsBySku = new Map(
        skus.map((sku) => [
            sku.id,
            parseSpecificationSignature(sku.specification_signature),
        ]),
    )
    const specNames = [
        ...new Set(
            [...parsedSpecsBySku.values()].flatMap((entries) =>
                entries.map((entry) => entry.attributeCode),
            ),
        ),
    ].sort((left, right) => left.localeCompare(right))
    const specs = specNames.map((name) => ({
        name,
        values: [
            ...new Set(
                [...parsedSpecsBySku.values()].flatMap((entries) =>
                    entries
                        .filter((entry) => entry.attributeCode === name)
                        .map((entry) => entry.valueCode),
                ),
            ),
        ],
    }))
    for (const sku of skus) {
        const skuRevisions = await fetchAllPages<SkuRevisionDto>(
            "/admin/sku-revisions",
            { sku_id: sku.id, sort_by: "revision_no", sort_dir: "desc" },
        ).catch(() => [] as SkuRevisionDto[])
        const rev = sku.current_revision_id
            ? skuRevisions.find(
                  (revision) => revision.id === sku.current_revision_id,
              )
            : undefined
        if (!rev) {
            throw new Error(
                `SKU ${sku.sku_no} 的当前修订不存在或已漂移，禁止编辑`,
            )
        }
        const unit = unitById.get(sku.base_unit_id)
        const parsedSpecs = parsedSpecsBySku.get(sku.id) ?? []
        const valuesByAttribute = new Map(
            parsedSpecs.map((entry) => [entry.attributeCode, entry.valueCode]),
        )
        const attributeValues = specNames.map(
            (name) => valuesByAttribute.get(name) ?? "",
        )
        const mainImageAssetId = rev?.source_main_image_asset_id?.trim()
        const mainAsset = mainImageAssetId
            ? await fetchFileAsset(mainImageAssetId)
            : null
        skuFields.push({
            skuId: sku.id,
            skuRevisionId: rev?.id,
            requiresExplicitReenable: asLifecycle(sku.status) === "DISABLED",
            specificationSignature: sku.specification_signature,
            skuNo: sku.sku_no,
            name: rev?.name ?? "",
            attributeValues,
            specLabel:
                (rev?.specification ??
                    parsedSpecs
                        .map(
                            (entry) =>
                                `${entry.attributeCode}：${entry.valueCode}`,
                        )
                        .join(" / ")) ||
                "默认规格",
            barcode: rev?.barcode ?? undefined,
            mainImage: mainAsset?.file_name ?? "",
            mainImagePreviewUrl: mainAsset?.public_url ?? undefined,
            mainImageAssetId: mainAsset?.id ?? undefined,
            salePrice: rev?.sales_visible_price_gross ?? undefined,
            marketPrice: rev?.market_price ?? undefined,
            baseUnit: unit?.name ?? unit?.symbol,
            listingStatus:
                sku.listing_status === "listed" ? "LISTED" : "UNLISTED",
            lifecycleStatus: asLifecycle(sku.status),
        })
    }

    const primaryUnit = skus[0] ? unitById.get(skus[0].base_unit_id) : undefined

    const category = categories.find(
        (item) => item.id === currentRev?.category_id,
    )
    const brand = brands.find((item) => item.id === currentRev?.brand_id)
    const productDetail = {
        lifecycleStatus: asLifecycle(product.status),
        productNo: product.product_no,
        description: currentRev?.description ?? undefined,
        specification: currentRev?.specification ?? undefined,
        baseUnitId: primaryUnit?.id ?? "",
        baseUnitCode: primaryUnit?.unit_code ?? "",
        baseUnit: primaryUnit?.name ?? primaryUnit?.symbol ?? "",
        categoryId: currentRev?.category_id ?? "",
        category: category?.name ?? "",
        brandId: currentRev?.brand_id ?? "",
        brand: brand?.name ?? "",
        carouselImages,
        detailImages,
        carouselPreviewUrls,
        detailPreviewUrls,
        carouselFileAssetIds,
        detailFileAssetIds,
        specs,
        skus: skuFields,
    }

    const row = mapProductRow(product, currentRev)
    const timeline: RevisionTimelineEntry[] = revisions.map((r) => ({
        id: r.id,
        revisionNo: r.revision_no,
        revisionTiming:
            r.id === currentRev?.id
                ? isFutureDate(r.effective_from)
                    ? ("FUTURE" as const)
                    : ("CURRENT" as const)
                : ("HISTORICAL" as const),
        timingLabel:
            r.id === currentRev?.id
                ? isFutureDate(r.effective_from)
                    ? "待生效"
                    : "当前生效"
                : "已结束",
        nameSnapshot: r.name,
        actor: "—",
        effectiveFrom: r.effective_from,
        effectiveTo: r.effective_to ?? undefined,
        changeReason: "—",
        isCurrent: r.id === currentRev?.id,
        lifecycleAtRevision: asLifecycle(r.status),
    }))

    return baseCenter("products", row, {
        productKind: product.product_kind,
        productDetail,
        productConstraints: {
            baseUnit: productDetail.baseUnit,
            hasFormalReferences: false,
            skuCount: skuFields.length,
        },
        revisionTimeline:
            timeline.length > 0
                ? timeline
                : baseCenter("products", row).revisionTimeline,
        currentRevision: {
            revisionId: currentRev?.id ?? product.id,
            revisionNo: currentRev?.revision_no ?? product.version,
            name: currentRev?.name ?? product.product_no,
            effectiveFrom:
                currentRev?.effective_from ??
                tsToIso(product.created_at).slice(0, 10),
            effectiveTo: currentRev?.effective_to ?? undefined,
            changeReason: "—",
            actor: "—",
            fields: row.keyFacts.map((f) => ({
                label: f.label,
                value: f.value,
            })),
        },
    })
}
