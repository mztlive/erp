import { apiGet } from "@/lib/api"
import type {
    ProductBrandDto,
    ProductCategoryDto,
    ProductDto,
    ProductRevisionDto,
    SellableSkuDto,
    SkuDto,
    SkuRevisionDto,
    SupplierDetailDto,
    SupplierQualificationDto,
    UnitOfMeasureDto,
    VoucherCategoryProfileDto,
    WarehouseDto,
    WarehouseRevisionDto,
} from "@/features/master-data/api/contracts"
import {
    fetchFileAsset,
    resolveMediaAssets,
} from "@/features/master-data/api/media-assets"
import {
    isFutureDate,
    mapBrandRow,
    mapCategoryRow,
    mapProductRow,
    mapSkuAsSellable,
    mapSupplierRow,
    mapUnitOfMeasureRow,
    mapVoucherRow,
    mapWarehouseRow,
} from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import {
    asLifecycle,
    capabilityLabel,
    fact,
    factsOf,
    invoiceLabel,
    isApiError,
    parseBusinessCategoryFromSnapshot,
    pickDefaultOrFirst,
    productKindLabel,
    ratingLabel,
    settlementLabel,
    taxRatePercent,
    tsToIso,
} from "@/features/master-data/api/presentation"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataResource,
    ProductSkuFields,
    RevisionTimelineEntry,
} from "@/features/master-data/types"

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

// ---------------------------------------------------------------------------
// Resource mappers · list
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// List fetchers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Center / detail
// ---------------------------------------------------------------------------

export function baseCenter(
    resource: MasterDataResource,
    row: MasterDataListItem,
    extras: Partial<MasterDataCenterView> = {},
): MasterDataCenterView {
    return {
        resource,
        stableId: row.stableId,
        stableNo: row.stableNo,
        name: row.name,
        lifecycleStatus: row.lifecycleStatus,
        lifecycleStatusLabel: row.lifecycleStatusLabel,
        lifecycleTone: row.lifecycleTone,
        scheduledLifecycleStatus: row.scheduledLifecycleStatus,
        scheduledLifecycleLabel: row.scheduledLifecycleLabel,
        revisionTiming: row.revisionTiming,
        revisionTimingLabel: row.revisionTimingLabel,
        lockVersion: row.lockVersion,
        currentRevision: {
            revisionId: row.currentRevisionId,
            revisionNo: row.revisionNo,
            name: row.name,
            effectiveFrom: row.effectiveFrom,
            effectiveTo: row.effectiveTo,
            changeReason: "—",
            actor: "—",
            fields: row.keyFacts.map((f) => ({
                label: f.label,
                value: f.value,
            })),
        },
        revisionTimeline: [
            {
                id: row.currentRevisionId,
                revisionNo: row.revisionNo,
                revisionTiming:
                    row.revisionTiming === "FUTURE" ? "FUTURE" : "CURRENT",
                timingLabel: row.revisionTimingLabel,
                nameSnapshot: row.name,
                actor: "—",
                effectiveFrom: row.effectiveFrom,
                effectiveTo: row.effectiveTo,
                changeReason: "—",
                isCurrent: true,
                lifecycleAtRevision: row.lifecycleStatus,
            },
        ],
        selectorEligibility: row.selectorEligibility,
        usageSummary: {
            historicalReferenceCount: 0,
            note: "引用摘要由后端投影提供；当前接口未返回业务引用数。",
        },
        sensitiveFields: [],
        resourceFacts: [...row.keyFacts],
        allowedActions: row.allowedActions,
        actionBlockers: row.actionBlockers,
        auditEvents: [],
        sections: ["overview", "versions", "relations", "audit"],
        ...extras,
    }
}

export async function centerCategory(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<ProductCategoryDto>(
        "/admin/product-categories",
        {},
    )
    const dto = items.find((c) => c.id === stableId)
    if (!dto) return null
    const byId = new Map(items.map((c) => [c.id, c]))
    const row = mapCategoryRow(dto)
    const parentName = dto.parent_category_id
        ? (byId.get(dto.parent_category_id)?.name ?? "（未知上级）")
        : "（根分类）"
    const facts = [
        { label: "分类代码", value: dto.category_code },
        { label: "上级分类", value: parentName },
        { label: "适用商品类型", value: productKindLabel(dto.product_kind) },
    ]
    return baseCenter(
        "categories",
        { ...row, keyFacts: facts },
        {
            resourceFacts: facts,
            currentRevision: {
                revisionId: dto.id,
                revisionNo: dto.version,
                name: dto.name,
                effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
                changeReason: "—",
                actor: "—",
                fields: facts,
            },
        },
    )
}

export async function centerBrand(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<ProductBrandDto>(
        "/admin/product-brands",
        {},
    )
    const dto = items.find((b) => b.id === stableId)
    if (!dto) return null
    const row = mapBrandRow(dto)
    const logoAssetId = dto.logo_asset_id?.trim()
    const logoAsset = logoAssetId ? await fetchFileAsset(logoAssetId) : null
    const logoUrl = logoAsset?.public_url?.trim()
    return baseCenter("brands", row, {
        resourceFacts: [
            { label: "品牌代码", value: dto.brand_code },
            {
                label: "品牌 Logo",
                value: logoUrl && logoAsset ? logoAsset.file_name : "—",
            },
        ],
        mediaAssets:
            logoUrl && logoAsset
                ? {
                      logo: [
                          {
                              fileName: logoAsset.file_name,
                              assetId: logoAssetId!,
                              url: logoUrl,
                          },
                      ],
                  }
                : undefined,
    })
}

export async function centerUnitOfMeasure(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<UnitOfMeasureDto>(
        "/admin/unit-of-measures",
        {},
    )
    const dto = items.find((u) => u.id === stableId)
    if (!dto) return null
    const row = mapUnitOfMeasureRow(dto)
    return baseCenter("unit-of-measures", row)
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

export async function centerSellable(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<SellableSkuDto>(
        "/admin/sellable-skus",
        {},
    )
    const item = items.find((candidate) => candidate.sku_id === stableId)
    if (!item) return null
    const row = mapSkuAsSellable(item)
    return baseCenter("sellable-items", row)
}

export async function centerVoucher(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const profiles = await fetchAllPages<VoucherCategoryProfileDto>(
        "/admin/voucher-category-profiles",
        {},
    )
    // stableId 为 SKU 身份；兼容旧链接仍按 profile.id 查找。
    const matched = profiles.filter(
        (p) => p.sku_id === stableId || p.id === stableId,
    )
    if (matched.length === 0) return null
    const profile = matched.reduce((best, cur) =>
        cur.revision_no > best.revision_no ? cur : best,
    )
    const skus = await fetchAllPages<SkuDto>("/admin/skus", {})
    const sku = skus.find((s) => s.id === profile.sku_id)
    const row = mapVoucherRow(profile, sku)
    return baseCenter("voucher-categories", row)
}

export async function centerWarehouse(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const warehouses = await fetchAllPages<WarehouseDto>(
        "/admin/warehouses",
        {},
    )
    const wh = warehouses.find((w) => w.id === stableId)
    if (!wh) return null
    const revisions = await fetchAllPages<WarehouseRevisionDto>(
        "/admin/warehouse-revisions",
        { warehouse_id: stableId, sort_by: "revision_no", sort_dir: "desc" },
    )
    const current = revisions[0]
    const row = mapWarehouseRow(wh, current)
    const timeline: RevisionTimelineEntry[] = revisions.map((r, index) => ({
        id: r.id,
        revisionNo: r.revision_no,
        revisionTiming:
            index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
        timingLabel: index === 0 ? "当前生效" : "已结束",
        nameSnapshot: r.name,
        actor: "—",
        effectiveFrom: r.effective_from,
        effectiveTo: r.effective_to ?? undefined,
        changeReason: r.change_reason,
        isCurrent: index === 0,
        lifecycleAtRevision: asLifecycle(wh.status),
    }))
    return baseCenter("warehouses", row, {
        warehouseStockSummary: {
            onHandQty: "—",
            reservedQty: "—",
            hasBlockingStock: false,
            w10Href: `/inventory?warehouseId=${encodeURIComponent(wh.id)}`,
            policyNote: "库存摘要由 W10 投影提供；当前接口未返回数量。",
        },
        revisionTimeline:
            timeline.length > 0
                ? timeline
                : baseCenter("warehouses", row).revisionTimeline,
        sensitiveFields: [
            {
                label: "联系人 / 地址",
                maskedValue: "（敏感字段，需授权查看）",
                visibility: "masked",
            },
        ],
    })
}

export async function centerSupplier(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    let detail: SupplierDetailDto
    try {
        detail = await apiGet<SupplierDetailDto>(`/admin/suppliers/${stableId}`)
    } catch (error) {
        if (isApiError(error) && error.status === 404) return null
        throw error
    }

    const profile = detail.current_profile
    const contacts = detail.contacts
    const banks = detail.bank_accounts
    const taxProfiles = detail.tax_profiles
    const capabilities = detail.capabilities
    const qualifications = detail.qualifications
    const ratings = detail.ratings
    const profiles = detail.commercial_profiles
    const partyName =
        detail.legal_name ||
        detail.short_name ||
        detail.party_no ||
        detail.supplier_no
    const row = mapSupplierRow(detail, partyName, profile)

    const contact = pickDefaultOrFirst(contacts)
    const bank = pickDefaultOrFirst(banks)
    const taxProfile = pickDefaultOrFirst(taxProfiles)
    const sortedRatings = [...ratings].sort(
        (a, b) => (b.revision_no ?? 0) - (a.revision_no ?? 0),
    )
    const rating = sortedRatings[0]
    const initialRating = [...sortedRatings]
        .reverse()
        .find((item) => item.initial_score != null)
    const invoiceTaxRatePercent = taxRatePercent(profile?.invoice_tax_rate)

    const capabilityLabels = capabilities
        .map((c) => capabilityLabel(c.capability_code))
        .filter(Boolean)
        .join("、")
    const capabilityCodeById = new Map(
        capabilities.map((capability) => [
            capability.id,
            capability.capability_code,
        ]),
    )
    const qualificationCapabilityCodes = Object.fromEntries(
        qualifications.map((qualification) => [
            `${qualification.qualification_type}::${qualification.certificate_no}`,
            qualification.capability_ids.flatMap((id) => {
                const code = capabilityCodeById.get(id)
                return code ? [code] : []
            }),
        ]),
    )

    // 经营类目：商务快照编码；兼容早期写入 capability.fulfillment_note 的数据
    const businessCategory =
        parseBusinessCategoryFromSnapshot(profile?.payment_term_snapshot) ||
        capabilities.map((c) => c.fulfillment_note?.trim()).find(Boolean) ||
        ""

    const qualByType = (type: string) =>
        qualifications.find((q) => q.qualification_type === type)

    // 资质附件：解析 asset → 文件清单（fileName/assetId/url），供回显链接与编辑回填
    const qualGroups = new Map<string, SupplierQualificationDto[]>()
    for (const q of qualifications) {
        const list = qualGroups.get(q.qualification_type) ?? []
        list.push(q)
        qualGroups.set(q.qualification_type, list)
    }
    const qualAssets = await resolveMediaAssets(
        qualifications
            .map((q) => q.attachment_id)
            .filter((id): id is string => Boolean(id?.trim())),
    )
    const qualFieldEntries = (
        type: string,
    ): { fileName: string; assetId: string; url: string }[] =>
        (qualGroups.get(type) ?? []).flatMap((q) => {
            const asset = q.attachment_id
                ? qualAssets.get(q.attachment_id)
                : null
            if (!q.attachment_id) return []
            return [
                {
                    fileName: asset?.file_name ?? q.certificate_no,
                    assetId: q.attachment_id,
                    url: asset?.public_url ?? "",
                },
            ]
        })
    const qualFileNames = (type: string): string =>
        qualFieldEntries(type)
            .map((entry) => entry.fileName)
            .join(", ")

    const contractQual = qualByType("contract")
    const authQual = qualByType("authorization")

    // 标签必须与 RESOURCE_FIELDS.suppliers / masterDataCopy 一致，供编辑回填
    const facts = factsOf(
        fact("供应商编号", detail.supplier_no),
        fact("企业主体", partyName),
        fact("统一社会信用代码", detail.unified_credit_code),
        fact("联系人", contact?.contact_name),
        // mobile 不在列表契约中；telephone 若创建时同步写入可回显
        fact("联系电话", contact?.telephone),
        fact("结算方式", settlementLabel(profile?.settlement_mode)),
        fact("发票类型", invoiceLabel(profile?.invoice_type)),
        fact("发票税点", invoiceTaxRatePercent),
        fact("能力", capabilityLabels),
        fact("经营类目", businessCategory || null),
        fact("公司签约主体", profile?.signing_entity_party_id),
        fact("公司付款主体", profile?.payment_entity_party_id),
        // 标签必须与 masterDataCopy / RESOURCE_FIELDS.suppliers 完全一致
        fact("资质附件", qualFileNames("certificate") || null),
        fact("合同编号", contractQual?.certificate_no),
        fact("合同有效期起", contractQual?.valid_from),
        fact("合同有效期止", contractQual?.valid_to),
        fact("合同文件", qualFileNames("contract") || null),
        fact("授权书文件", qualFileNames("authorization") || null),
        fact("授权书有效期起", authQual?.valid_from),
        fact("授权书有效期止", authQual?.valid_to),
        fact("食品经营许可证", qualFileNames("food_license") || null),
        fact("供应商法人身份证", qualFileNames("legal_person_id") || null),
        fact("税号", taxProfile?.tax_no),
        fact("开户银行", bank?.bank_name),
        // 银行账号明文不在列表契约中，无法回显
        fact("供应商评级", ratingLabel(rating?.rating)),
        fact(
            "合作期初评分",
            initialRating?.initial_score != null
                ? String(initialRating.initial_score)
                : null,
        ),
        fact(
            "合作中评分",
            rating?.current_score != null ? String(rating.current_score) : null,
        ),
    )

    // 展示用摘要（含无值占位），与编辑 fields 分离
    const displayFacts = [
        { label: "供应商编号", value: detail.supplier_no },
        { label: "企业主体", value: partyName || "—" },
        { label: "联系人", value: contact?.contact_name || "—" },
        { label: "联系电话", value: contact?.telephone || "—" },
        {
            label: "结算方式",
            value: settlementLabel(profile?.settlement_mode) || "—",
        },
        {
            label: "发票类型",
            value: invoiceLabel(profile?.invoice_type) || "—",
        },
        {
            label: "发票税点",
            value: invoiceTaxRatePercent ? `${invoiceTaxRatePercent}%` : "—",
        },
        { label: "能力", value: capabilityLabels || "—" },
        {
            label: "资质",
            value:
                qualifications.length > 0 ? `${qualifications.length} 项` : "—",
        },
        {
            label: "供应商评级",
            value: ratingLabel(rating?.rating) || "—",
        },
        {
            label: "税号",
            value: taxProfile?.tax_no ?? "—",
        },
        { label: "开户银行", value: bank?.bank_name || "—" },
    ]

    const timeline: RevisionTimelineEntry[] = profiles.map((p, index) => ({
        id: p.id,
        revisionNo: p.revision_no,
        revisionTiming:
            index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
        timingLabel: index === 0 ? "当前生效" : "已结束",
        nameSnapshot: partyName,
        actor: "—",
        effectiveFrom: tsToIso(p.created_at).slice(0, 10),
        changeReason: p.change_reason,
        isCurrent: index === 0,
        lifecycleAtRevision: asLifecycle(detail.status),
    }))

    return baseCenter("suppliers", row, {
        partyLockVersion: detail.party_version ?? undefined,
        supplierQualificationCapabilityCodes: qualificationCapabilityCodes,
        resourceFacts: displayFacts,
        currentRevision: {
            revisionId: profile?.id ?? detail.id,
            revisionNo: profile?.revision_no ?? detail.version,
            name: partyName,
            effectiveFrom: tsToIso(
                profile?.created_at ?? detail.created_at,
            ).slice(0, 10),
            changeReason: profile?.change_reason ?? "—",
            actor: "—",
            // 编辑回填专用：完整字段 + 真实值（无「—」占位）
            fields: facts,
        },
        mediaAssets: {
            qualification: qualFieldEntries("certificate"),
            contractFile: qualFieldEntries("contract"),
            authorizationFile: qualFieldEntries("authorization"),
            foodLicense: qualFieldEntries("food_license"),
            legalPersonIdCard: qualFieldEntries("legal_person_id"),
        },
        revisionTimeline:
            timeline.length > 0
                ? timeline
                : baseCenter("suppliers", row).revisionTimeline,
        sensitiveFields: detail.sensitive_fields.map((field) => ({
            label: field.label,
            maskedValue: field.masked_value,
            revealToken: field.reveal_token,
            visibility: "masked" as const,
        })),
    })
}
