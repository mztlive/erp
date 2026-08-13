import { apiPost, apiPut } from "@/lib/api"
import {
    WAREHOUSE_WRITE_CODE,
    WAREHOUSE_WRITE_MESSAGE,
    resourceLabel,
} from "@/features/master-data/lib/data"
import type {
    BrandFields,
    CategoryFields,
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
    ProductFields,
    ProductKind,
    SellableItemFields,
    SupplierFields,
    UnitOfMeasureFields,
    VoucherCategoryFields,
} from "@/features/master-data/types"
import {
    ProductBrandDto,
    ProductCategoryDto,
    ProductDto,
    SupplierDto,
    SupplierProfileMutationDto,
    UnitOfMeasureDto,
    VoucherCategoryProfileDto,
} from "@/features/master-data/api/contracts"
import { isFutureDate } from "@/features/master-data/api/list-mappers"
import { centerProduct } from "@/features/master-data/api/centers"
import {
    buildPaymentTermSnapshot,
    capabilityToBackend,
    genBusinessCode,
    invoiceToBackend,
    isApiError,
    isoNow,
    normalizeTaxRate,
    parseScore100,
    ratingToBackend,
    settlementToBackend,
    todayDateOnly,
    tsToIso,
} from "@/features/master-data/api/presentation"
import { parseMediaList } from "@/features/master-data/lib/resource-fields"

function blockedWarehouse(): MasterDataMutationResult {
    return {
        outcome: "blocked",
        code: WAREHOUSE_WRITE_CODE,
        message: WAREHOUSE_WRITE_MESSAGE,
        detail: "仓库资料暂不可维护，任何角色都不能改。",
    }
}

function mapMutationError(
    error: unknown,
    fallbackLock?: { version: number; revisionNo: number },
): MasterDataMutationResult {
    if (!isApiError(error)) {
        throw error
    }
    if (error.status === 409) {
        return {
            outcome: "conflict",
            message: "资料已被他人更新，请刷新后重新填写。",
            serverLockVersion: fallbackLock?.version ?? 0,
            serverRevisionNo: fallbackLock?.revisionNo ?? 0,
        }
    }
    if (
        error.kind === "Validation" ||
        error.status === 400 ||
        error.status === 422
    ) {
        return {
            outcome: "blocked",
            code: "VALIDATION",
            message: error.message || "请求未通过业务校验",
        }
    }
    // Let network/auth/5xx propagate for Query error state
    throw error
}

async function createCategory(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as CategoryFields
    try {
        const created = await apiPost<ProductCategoryDto>(
            "/admin/product-categories",
            {
                category_code: fields.code,
                parent_category_id: fields.parentId || null,
                name: input.name.trim(),
                product_kind: mapProductKindInput(fields.productKind),
                status: "active",
            },
        )
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.category_code,
            revisionId: created.id,
            revisionNo: created.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.category_code}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

function mapProductKindInput(kind: string | undefined): ProductKind {
    if (
        kind === "PHYSICAL" ||
        kind === "VIRTUAL" ||
        kind === "OFFLINE_SERVICE" ||
        kind === "VOUCHER"
    ) {
        return kind
    }
    // Chinese labels from category form
    switch (kind) {
        case "实物":
            return "PHYSICAL"
        case "虚拟":
            return "VIRTUAL"
        case "服务":
        case "线下服务":
            return "OFFLINE_SERVICE"
        case "卡券":
            return "VOUCHER"
        default:
            return "PHYSICAL"
    }
}

async function createBrand(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as BrandFields
    try {
        const created = await apiPost<ProductBrandDto>(
            "/admin/product-brands",
            {
                brand_code: fields.code,
                name: input.name.trim(),
                status: "active",
                logo_file_asset_id: fields.logoAssetId || null,
            },
        )
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.brand_code,
            revisionId: created.id,
            revisionNo: created.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.brand_code}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

function parseQuantityScale(raw: string | undefined): number | null {
    const value = Number((raw ?? "").trim())
    if (!Number.isInteger(value) || value < 0 || value > 6) return null
    return value
}

async function createUnitOfMeasure(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as UnitOfMeasureFields
    const quantityScale = parseQuantityScale(fields.quantityScale)
    if (quantityScale === null) {
        return {
            outcome: "blocked",
            code: "UNIT_QUANTITY_SCALE_INVALID",
            message: "数量小数位必须是 0–6 的整数。",
        }
    }
    if (!fields.code.trim()) {
        return {
            outcome: "blocked",
            code: "UNIT_CODE_REQUIRED",
            message: "请填写单位代码。",
        }
    }
    if (!fields.symbol.trim()) {
        return {
            outcome: "blocked",
            code: "UNIT_SYMBOL_REQUIRED",
            message: "请填写单位符号。",
        }
    }
    try {
        const created = await apiPost<UnitOfMeasureDto>(
            "/admin/unit-of-measures",
            {
                unit_code: fields.code.trim(),
                name: input.name.trim(),
                symbol: fields.symbol.trim(),
                quantity_scale: quantityScale,
                status: "active",
            },
        )
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.unit_code,
            revisionId: created.id,
            revisionNo: created.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.unit_code}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

/** SPU 媒体写入项：文件资产 + 展示顺序。 */
function mapProductMedia(
    names: readonly string[],
    assetIds: Readonly<Record<string, string>>,
): Array<{ file_asset_id: string; sort_order: number }> {
    return names
        .map((name, index) => ({
            file_asset_id: assetIds[name]?.trim() ?? "",
            sort_order: index,
        }))
        .filter((entry) => entry.file_asset_id)
}

function mapProductSkus(fields: ProductFields) {
    return fields.skus
        .filter((sku) => sku.lifecycleStatus === "ENABLED")
        .map((sku) => ({
            sku_id: sku.skuId || null,
            expected_sku_revision_id: sku.skuRevisionId || null,
            reenable: Boolean(sku.skuId && sku.requiresExplicitReenable),
            sku_no: sku.skuNo,
            name: sku.name.trim(),
            base_unit_id: fields.baseUnitId,
            barcode: sku.barcode || null,
            main_image_asset_id: sku.mainImageAssetId || null,
            weight_kg: null,
            volume_m3: null,
            sales_visible_price_gross: sku.salePrice || null,
            market_price: sku.marketPrice || null,
            spec_entries: fields.specs.flatMap((spec, index) => {
                const attributeCode = spec.name.trim()
                const attributeValueCode = (
                    sku.attributeValues[index] ?? ""
                ).trim()
                return attributeCode && attributeValueCode
                    ? [
                          {
                              attribute_code: attributeCode,
                              attribute_value_code: attributeValueCode,
                          },
                      ]
                    : []
            }),
        }))
}

async function createProduct(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as ProductFields
    if (!fields.productKind) {
        return {
            outcome: "blocked",
            code: "PRODUCT_KIND_REQUIRED",
            message: "请选择商品类型后再保存。",
            detail: "商品类型决定商品业务作用，保存后不可修改。",
        }
    }
    if (!fields.categoryId || !fields.brandId || !fields.baseUnitId) {
        return {
            outcome: "blocked",
            code: "PRODUCT_REQUIRED_REFS",
            message: "请完整填写分类、品牌与基础单位。",
        }
    }
    if (fields.skus.length === 0) {
        return {
            outcome: "blocked",
            code: "SKU_REQUIRED",
            message: "至少需要一个 SKU。",
        }
    }

    try {
        const created = await apiPost<ProductDto>("/admin/products", {
            change_reason: input.changeReason || "新建商品",
            product_no: fields.productNo.trim(),
            product_kind: fields.productKind,
            name: input.name.trim(),
            description: fields.description || null,
            specification: fields.specification || null,
            category_id: fields.categoryId,
            brand_id: fields.brandId,
            status: "active",
            effective_from: input.effectiveFrom,
            effective_to: input.effectiveTo || null,
            carousel_media: mapProductMedia(
                fields.carouselImages,
                fields.carouselFileAssetIds,
            ),
            detail_media: mapProductMedia(
                fields.detailImages,
                fields.detailFileAssetIds,
            ),
            skus: mapProductSkus(fields),
        })
        if (!created.current_revision_id) {
            throw new Error("商品创建成功但未返回当前修订，禁止伪造修订身份")
        }
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.product_no,
            revisionId: created.current_revision_id,
            revisionNo: 1,
            revisionState: isFutureDate(input.effectiveFrom)
                ? "FUTURE"
                : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.product_no}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

async function createVoucherCategory(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as VoucherCategoryFields
    // 分类 / 品牌 / 单位均可省略：后端补齐共用卡券根分类、品牌「福尚云」、单位「张」。
    // 若调用方仍传入 categoryId / newCategory / brandId / baseUnitId，则原样转发覆盖默认。
    const body: Record<string, unknown> = {
        voucher_no: fields.voucherNo,
        name: input.name.trim(),
        description: (fields.description || input.name).trim(),
        specification: fields.specification || null,
        status: "active",
        effective_from: input.effectiveFrom || null,
        effective_to: input.effectiveTo || null,
    }
    if (fields.categoryId) {
        body.category_id = fields.categoryId
    } else if (fields.newCategoryCode && fields.newCategoryName) {
        body.new_category = {
            category_code: fields.newCategoryCode,
            parent_category_id: fields.newCategoryParentId || null,
            name: fields.newCategoryName,
        }
    }
    if (fields.brandId) {
        body.brand_id = fields.brandId
    }
    if (fields.baseUnitId) {
        body.sku = {
            base_unit_id: fields.baseUnitId,
            barcode: fields.barcode || null,
            weight_kg: null,
            volume_m3: null,
            sales_visible_price_gross: fields.salesVisiblePriceGross || null,
            market_price: fields.marketPrice || null,
        }
    }
    try {
        const created = await apiPost<VoucherCategoryProfileDto>(
            "/admin/voucher-categories",
            body,
        )
        return {
            outcome: "succeeded",
            stableId: created.sku_id,
            stableNo: created.sku_no ?? fields.voucherNo,
            revisionId: created.id,
            revisionNo: created.revision_no,
            revisionState: isFutureDate(input.effectiveFrom)
                ? "FUTURE"
                : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-VC-${fields.voucherNo}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

type SupplierProfileQualificationInput = {
    qualification_type: string
    certificate_no: string
    issuer: null
    valid_from: string
    valid_to: string | null
    attachment_id: string | null
    capability_codes: string[]
}

/** 将页面文件字段转换为根级供应商资料命令中的结构化资质集合。 */
const buildSupplierProfileQualifications = (
    fields: SupplierFields,
    effectiveFrom: string,
    capabilityCodes: string[],
): SupplierProfileQualificationInput[] => {
    const result: SupplierProfileQualificationInput[] = []
    const pushFiles = (
        qualificationType: string,
        names: string[],
        assetIds: Readonly<Record<string, string>> | undefined,
        validFrom: string,
        validTo?: string,
        certificateNo?: string,
    ) => {
        names.forEach((name, index) => {
            const attachmentId = assetIds?.[name]?.trim()
            if (!attachmentId) return
            const resolvedCertificateNo =
                certificateNo && index === 0 ? certificateNo : name
            const existingCodes =
                fields.qualificationCapabilityCodes?.[
                    `${qualificationType}::${resolvedCertificateNo}`
                ]
            result.push({
                qualification_type: qualificationType,
                certificate_no: resolvedCertificateNo,
                issuer: null,
                valid_from: validFrom,
                valid_to: validTo || null,
                attachment_id: attachmentId,
                capability_codes: existingCodes
                    ? existingCodes.filter((code) =>
                          capabilityCodes.includes(code),
                      )
                    : capabilityCodes,
            })
        })
    }
    pushFiles(
        "certificate",
        parseMediaList(fields.qualification),
        fields.qualificationFileAssetIds,
        effectiveFrom,
    )
    pushFiles(
        "contract",
        parseMediaList(fields.contractFile).slice(0, 1),
        fields.contractFileAssetIds,
        fields.contractValidFrom || effectiveFrom,
        fields.contractValidTo,
        fields.contractNo?.trim() || "CONTRACT",
    )
    pushFiles(
        "authorization",
        parseMediaList(fields.authorizationFile),
        fields.authorizationFileAssetIds,
        fields.authorizationValidFrom || effectiveFrom,
        fields.authorizationValidTo,
    )
    pushFiles(
        "food_license",
        parseMediaList(fields.foodLicense),
        fields.foodLicenseFileAssetIds,
        effectiveFrom,
    )
    pushFiles(
        "legal_person_id",
        parseMediaList(fields.legalPersonIdCard),
        fields.legalPersonIdCardFileAssetIds,
        effectiveFrom,
    )
    return result
}

async function createSupplier(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as SupplierFields
    const capabilityCodes = (fields.capability ?? "")
        .split(/[、,，]/)
        .map((value) => capabilityToBackend(value.trim()))
        .filter((value): value is string => Boolean(value))
    const effectiveFrom = input.effectiveFrom || todayDateOnly()
    if (!fields.signingEntity?.trim() || !fields.paymentEntity?.trim()) {
        return {
            outcome: "blocked",
            code: "SUPPLIER_INTERNAL_PARTY_REQUIRED",
            message: "请选择公司签约主体和公司付款主体。",
        }
    }
    try {
        const created = await apiPost<SupplierProfileMutationDto>(
            "/admin/supplier-profiles",
            {
                idempotency_key: input.idempotencyKey,
                party_no: genBusinessCode("PTY"),
                supplier_no: genBusinessCode("SUP"),
                expected_party_version: null,
                expected_supplier_version: null,
                legal_name: fields.company || input.name.trim(),
                short_name: input.name.trim(),
                unified_credit_code: fields.creditCode?.trim() || null,
                contact:
                    fields.contactName?.trim() && fields.contactPhone?.trim()
                        ? {
                              contact_name: fields.contactName.trim(),
                              mobile: fields.contactPhone.trim(),
                              telephone: null,
                              email: null,
                          }
                        : null,
                clear_contact: false,
                address: fields.address?.trim()
                    ? {
                          address: fields.address.trim(),
                          contact_name: fields.contactName?.trim() || null,
                      }
                    : null,
                clear_address: false,
                tax_no: fields.taxNo?.trim() || null,
                clear_tax_profile: false,
                bank_account:
                    fields.bankName?.trim() && fields.bankAccount?.trim()
                        ? {
                              bank_name: fields.bankName.trim(),
                              account_number: fields.bankAccount.trim(),
                          }
                        : null,
                clear_bank_account: false,
                settlement_mode: settlementToBackend(fields.settlement),
                reconciliation_cycle: "monthly",
                payment_term_snapshot: buildPaymentTermSnapshot(
                    fields.settlement,
                    fields.businessCategory,
                ),
                invoice_type: invoiceToBackend(fields.invoiceType),
                invoice_tax_rate: normalizeTaxRate(fields.invoiceTaxRate),
                signing_entity_party_id: fields.signingEntity.trim(),
                payment_entity_party_id: fields.paymentEntity.trim(),
                capability_codes: capabilityCodes,
                qualifications: buildSupplierProfileQualifications(
                    fields,
                    effectiveFrom,
                    capabilityCodes,
                ),
                rating:
                    fields.supplierRating ||
                    fields.currentScore ||
                    fields.initialScore
                        ? {
                              initial_score:
                                  parseScore100(fields.initialScore) ?? null,
                              rating: ratingToBackend(fields.supplierRating),
                              current_score:
                                  parseScore100(fields.currentScore) ?? 0,
                              valid_from: effectiveFrom,
                          }
                        : null,
                effective_from: effectiveFrom,
                change_reason: input.changeReason || "新建",
            },
        )

        return {
            outcome: "succeeded",
            stableId: created.supplier_id,
            stableNo: created.supplier_no,
            revisionId: created.revision_id,
            revisionNo: created.revision_no,
            revisionState: "CURRENT",
            effectiveFrom: created.effective_from,
            recordedAt: tsToIso(created.recorded_at),
            actor: "—",
            changeReason: created.change_reason,
            reference: `MD-CREATE-${created.supplier_no}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

async function createSellable(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    // Sellable pool is a projection over company SKUs; not an independent create target.
    // Treat as create product with single SKU is wrong domain. Block with guidance.
    const fields = input.fields as SellableItemFields
    void fields
    return {
        outcome: "blocked",
        code: "SELLABLE_NOT_WRITABLE",
        message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
        detail: "W14：sellable-items 不是独立 resource 写入口。",
    }
}

export async function createMasterDataObject(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    if (input.resource === "warehouses") return blockedWarehouse()
    switch (input.resource) {
        case "categories":
            return createCategory(input)
        case "brands":
            return createBrand(input)
        case "unit-of-measures":
            return createUnitOfMeasure(input)
        case "products":
            return createProduct(input)
        case "voucher-categories":
            return createVoucherCategory(input)
        case "suppliers":
            return createSupplier(input)
        case "sellable-items":
            return createSellable(input)
        default:
            return {
                outcome: "blocked",
                code: "UNSUPPORTED_RESOURCE",
                message: `暂不支持新建资源：${resourceLabel(input.resource)}`,
            }
    }
}

export async function createMasterDataRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    if (input.resource === "warehouses") return blockedWarehouse()

    try {
        switch (input.resource) {
            case "categories": {
                const fields = input.fields as CategoryFields
                const updated = await apiPut<ProductCategoryDto>(
                    `/admin/product-categories/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        name: input.name.trim(),
                        product_kind: fields.productKind
                            ? mapProductKindInput(fields.productKind)
                            : undefined,
                        status: undefined,
                    },
                )
                // parent move is a separate endpoint
                if (fields.parentId !== undefined) {
                    try {
                        await apiPut(
                            `/admin/product-categories/${input.stableId}/parent`,
                            {
                                version: updated.version,
                                parent_category_id: fields.parentId || null,
                            },
                        )
                    } catch (error) {
                        return mapMutationError(error, {
                            version: updated.version,
                            revisionNo: updated.version,
                        })
                    }
                }
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.category_code,
                    revisionId: updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-REV-${updated.category_code}-v${updated.version}`,
                    nextActions: ["查看变更历史", "返回列表"],
                }
            }
            case "brands": {
                const fields = input.fields as BrandFields
                const updated = await apiPut<ProductBrandDto>(
                    `/admin/product-brands/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        name: input.name.trim(),
                        logo_file_asset_id: fields.logo
                            ? fields.logoAssetId || null
                            : null,
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.brand_code,
                    revisionId: updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-REV-${updated.brand_code}-v${updated.version}`,
                    nextActions: ["查看变更历史", "返回列表"],
                }
            }
            case "unit-of-measures": {
                const fields = input.fields as UnitOfMeasureFields
                const quantityScale = parseQuantityScale(fields.quantityScale)
                if (quantityScale === null) {
                    return {
                        outcome: "blocked",
                        code: "UNIT_QUANTITY_SCALE_INVALID",
                        message: "数量小数位必须是 0–6 的整数。",
                    }
                }
                if (!fields.symbol.trim()) {
                    return {
                        outcome: "blocked",
                        code: "UNIT_SYMBOL_REQUIRED",
                        message: "请填写单位符号。",
                    }
                }
                const updated = await apiPut<UnitOfMeasureDto>(
                    `/admin/unit-of-measures/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        name: input.name.trim(),
                        symbol: fields.symbol.trim(),
                        quantity_scale: quantityScale,
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.unit_code,
                    revisionId: updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-REV-${updated.unit_code}-v${updated.version}`,
                    nextActions: ["查看变更历史", "返回列表"],
                }
            }
            case "products": {
                const fields = input.fields as ProductFields
                if (
                    !fields.categoryId ||
                    !fields.brandId ||
                    !fields.baseUnitId
                ) {
                    return {
                        outcome: "blocked",
                        code: "PRODUCT_REQUIRED_REFS",
                        message: "请完整填写分类、品牌与基础单位。",
                    }
                }
                const updated = await apiPut<ProductDto>(
                    `/admin/products/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        change_reason: input.changeReason,
                        name: input.name.trim(),
                        description: fields.description || null,
                        specification: fields.specification || null,
                        category_id: fields.categoryId,
                        brand_id: fields.brandId,
                        status:
                            fields.lifecycleStatus === "DISABLED"
                                ? "disabled"
                                : "active",
                        effective_from: input.effectiveFrom,
                        effective_to: input.effectiveTo || null,
                        carousel_media: mapProductMedia(
                            fields.carouselImages,
                            fields.carouselFileAssetIds,
                        ),
                        detail_media: mapProductMedia(
                            fields.detailImages,
                            fields.detailFileAssetIds,
                        ),
                        skus: mapProductSkus(fields),
                    },
                )
                if (!updated.current_revision_id) {
                    throw new Error(
                        "商品更新成功但未返回当前修订，禁止伪造修订身份",
                    )
                }
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.product_no,
                    revisionId: updated.current_revision_id,
                    revisionNo: updated.version,
                    revisionState: isFutureDate(input.effectiveFrom)
                        ? "FUTURE"
                        : "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-REV-${updated.product_no}-v${updated.version}`,
                    nextActions: ["查看变更历史", "返回列表"],
                }
            }
            case "suppliers": {
                const fields = input.fields as SupplierFields
                const capabilityCodes = (fields.capability ?? "")
                    .split(/[、,，]/)
                    .map((value) => capabilityToBackend(value.trim()))
                    .filter((value): value is string => Boolean(value))
                const effectiveFrom = input.effectiveFrom || todayDateOnly()
                if (
                    input.expectedPartyVersion == null ||
                    !fields.signingEntity?.trim() ||
                    !fields.paymentEntity?.trim()
                ) {
                    return {
                        outcome: "blocked",
                        code: "SUPPLIER_PROFILE_REQUIRED_CONTEXT",
                        message:
                            "供应商版本或签约、付款主体缺失，请刷新后重试。",
                    }
                }
                const updated = await apiPut<SupplierProfileMutationDto>(
                    `/admin/supplier-profiles/${input.stableId}`,
                    {
                        idempotency_key: input.idempotencyKey,
                        party_no: null,
                        supplier_no: null,
                        expected_party_version: input.expectedPartyVersion,
                        expected_supplier_version: input.expectedLockVersion,
                        legal_name: fields.company || input.name.trim(),
                        short_name: input.name.trim(),
                        unified_credit_code: fields.creditCode?.trim() || null,
                        contact:
                            fields.contactName?.trim() &&
                            fields.contactPhone?.trim()
                                ? {
                                      contact_name: fields.contactName.trim(),
                                      mobile: fields.contactPhone.trim(),
                                      telephone: null,
                                      email: null,
                                  }
                                : null,
                        clear_contact: fields.clearContact === true,
                        address: fields.address?.trim()
                            ? {
                                  address: fields.address.trim(),
                                  contact_name:
                                      fields.contactName?.trim() || null,
                              }
                            : null,
                        clear_address: fields.clearAddress === true,
                        tax_no: fields.taxNo?.trim() || null,
                        clear_tax_profile: fields.clearTaxProfile === true,
                        bank_account:
                            fields.bankName?.trim() &&
                            fields.bankAccount?.trim()
                                ? {
                                      bank_name: fields.bankName.trim(),
                                      account_number: fields.bankAccount.trim(),
                                  }
                                : null,
                        clear_bank_account: fields.clearBankAccount === true,
                        settlement_mode: settlementToBackend(fields.settlement),
                        reconciliation_cycle: "monthly",
                        payment_term_snapshot: buildPaymentTermSnapshot(
                            fields.settlement,
                            fields.businessCategory,
                        ),
                        invoice_type: invoiceToBackend(fields.invoiceType),
                        invoice_tax_rate: normalizeTaxRate(
                            fields.invoiceTaxRate,
                        ),
                        signing_entity_party_id: fields.signingEntity.trim(),
                        payment_entity_party_id: fields.paymentEntity.trim(),
                        capability_codes: capabilityCodes,
                        qualifications: buildSupplierProfileQualifications(
                            fields,
                            effectiveFrom,
                            capabilityCodes,
                        ),
                        rating:
                            fields.supplierRating ||
                            fields.currentScore ||
                            fields.initialScore
                                ? {
                                      initial_score:
                                          parseScore100(fields.initialScore) ??
                                          null,
                                      rating: ratingToBackend(
                                          fields.supplierRating,
                                      ),
                                      current_score:
                                          parseScore100(fields.currentScore) ??
                                          0,
                                      valid_from: effectiveFrom,
                                  }
                                : null,
                        effective_from: effectiveFrom,
                        change_reason: input.changeReason,
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.supplier_id,
                    stableNo: updated.supplier_no,
                    revisionId: updated.revision_id,
                    revisionNo: updated.revision_no,
                    revisionState: "CURRENT",
                    effectiveFrom: updated.effective_from,
                    recordedAt: tsToIso(updated.recorded_at),
                    actor: "—",
                    changeReason: updated.change_reason,
                    reference: `MD-REV-${updated.supplier_no}-v${updated.revision_no}`,
                    nextActions: ["查看变更历史", "返回列表"],
                }
            }
            case "sellable-items":
                return {
                    outcome: "blocked",
                    code: "SELLABLE_NOT_WRITABLE",
                    message:
                        "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
                }
            case "voucher-categories": {
                const fields = input.fields as VoucherCategoryFields
                try {
                    const updated = await apiPut<VoucherCategoryProfileDto>(
                        `/admin/voucher-categories/${input.stableId}`,
                        {
                            version: input.expectedLockVersion,
                            name: input.name.trim(),
                            description: (
                                fields.description || input.name
                            ).trim(),
                            effective_from: input.effectiveFrom || null,
                            effective_to: input.effectiveTo || null,
                        },
                    )
                    return {
                        outcome: "succeeded",
                        stableId: updated.sku_id,
                        stableNo:
                            updated.sku_no ??
                            fields.voucherNo ??
                            input.stableId,
                        revisionId: updated.id,
                        revisionNo: updated.revision_no,
                        revisionState: isFutureDate(input.effectiveFrom)
                            ? "FUTURE"
                            : "CURRENT",
                        effectiveFrom: input.effectiveFrom,
                        recordedAt: isoNow(),
                        actor: "—",
                        changeReason: input.changeReason || "更新",
                        reference: `MD-REV-VC-${updated.sku_no ?? input.stableId}-v${updated.revision_no}`,
                        nextActions: ["返回列表"],
                    }
                } catch (error) {
                    return mapMutationError(error, {
                        version: input.expectedLockVersion,
                        revisionNo: 0,
                    })
                }
            }
            default:
                return {
                    outcome: "blocked",
                    code: "UNSUPPORTED_RESOURCE",
                    message: `暂不支持更新资源：${resourceLabel(input.resource)}`,
                }
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableMasterDataObject(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    if (input.resource === "warehouses") return blockedWarehouse()

    try {
        switch (input.resource) {
            case "categories": {
                const updated = await apiPut<ProductCategoryDto>(
                    `/admin/product-categories/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        status: "disabled",
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.category_code,
                    revisionId: updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-DIS-${updated.category_code}`,
                    nextActions: ["返回列表"],
                }
            }
            case "brands": {
                const updated = await apiPut<ProductBrandDto>(
                    `/admin/product-brands/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        status: "disabled",
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.brand_code,
                    revisionId: updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-DIS-${updated.brand_code}`,
                    nextActions: ["返回列表"],
                }
            }
            case "unit-of-measures": {
                const updated = await apiPut<UnitOfMeasureDto>(
                    `/admin/unit-of-measures/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        status: "disabled",
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.unit_code,
                    revisionId: updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-DIS-${updated.unit_code}`,
                    nextActions: ["返回列表"],
                }
            }
            case "products": {
                // Product update requires full body; load current then set disabled.
                const center = await centerProduct(input.stableId)
                if (!center) {
                    return {
                        outcome: "unknown",
                        message: "资料不存在或无权访问。",
                        idempotencyKey: input.idempotencyKey,
                    }
                }
                if (center.lifecycleStatus === "DISABLED") {
                    return {
                        outcome: "blocked",
                        code: "ALREADY_DISABLED",
                        message: "资料已停用；不是删除，历史记录仍可查看。",
                    }
                }
                const detail = center.productDetail
                const updated = await apiPut<ProductDto>(
                    `/admin/products/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        change_reason: input.changeReason,
                        name: center.name,
                        description: detail?.description || null,
                        specification: detail?.specification || null,
                        category_id: detail?.categoryId || "",
                        brand_id: detail?.brandId || "",
                        status: "disabled",
                        effective_from: input.effectiveFrom,
                        effective_to:
                            center.currentRevision.effectiveTo || null,
                        carousel_media: mapProductMedia(
                            detail?.carouselImages ?? [],
                            detail?.carouselFileAssetIds ?? {},
                        ),
                        detail_media: mapProductMedia(
                            detail?.detailImages ?? [],
                            detail?.detailFileAssetIds ?? {},
                        ),
                        skus: detail
                            ? mapProductSkus({
                                  ...detail,
                                  productKind: center.productKind ?? "",
                              })
                            : [],
                    },
                )
                if (!updated.current_revision_id) {
                    throw new Error(
                        "商品停用成功但未返回当前修订，禁止伪造修订身份",
                    )
                }
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.product_no,
                    revisionId: updated.current_revision_id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-DIS-${updated.product_no}`,
                    nextActions: ["返回列表"],
                }
            }
            case "suppliers": {
                const updated = await apiPut<SupplierDto>(
                    `/admin/suppliers/${input.stableId}`,
                    {
                        version: input.expectedLockVersion,
                        status: "disabled",
                    },
                )
                return {
                    outcome: "succeeded",
                    stableId: updated.id,
                    stableNo: updated.supplier_no,
                    revisionId:
                        updated.current_commercial_profile_revision_id ??
                        updated.id,
                    revisionNo: updated.version,
                    revisionState: "CURRENT",
                    effectiveFrom: input.effectiveFrom,
                    recordedAt: isoNow(),
                    actor: "—",
                    changeReason: input.changeReason,
                    reference: `MD-DIS-${updated.supplier_no}`,
                    nextActions: ["返回列表"],
                }
            }
            case "voucher-categories":
                return {
                    outcome: "blocked",
                    code: "VOUCHER_NO_DISABLE",
                    message: "卡券类目不支持停用。",
                }
            case "sellable-items":
                return {
                    outcome: "blocked",
                    code: "SELLABLE_NOT_WRITABLE",
                    message:
                        "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
                }
            default:
                return {
                    outcome: "blocked",
                    code: "UNSUPPORTED_RESOURCE",
                    message: `暂不支持停用资源：${resourceLabel(input.resource)}`,
                }
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

/** 使用短期令牌揭示供应商敏感字段；服务端再次执行权限校验并记录审计。 */
export async function revealMasterDataSensitive(
    revealToken: string,
): Promise<string> {
    const result = await apiPost<{ value: string }>(
        "/admin/supplier-sensitive-fields/reveal",
        { reveal_token: revealToken },
    )
    return result.value
}

// Re-export pure display helpers used by pages (stable import path via queries)
