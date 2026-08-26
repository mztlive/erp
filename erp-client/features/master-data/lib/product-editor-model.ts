import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import {
    emptyProductFields,
    rebuildSkusFromSpecs,
    validateProductFields,
} from "@/features/master-data/lib/product-model"
import type {
    MasterDataCenterView,
    ProductDetailView,
    ProductFields,
    ProductSpecDimension,
} from "@/features/master-data/types"

function newIdempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

type ProductSpecDraft = Readonly<{
    name: string
    values: readonly string[]
}>

type ProductEditorFormValues = Readonly<{
    name: string
    effectiveFrom: string
    effectiveTo: string
    changeReason: string
    fields: ProductFields
    specDrafts: readonly ProductSpecDraft[]
    batchSalePrice: string
    batchMarketPrice: string
}>

type ProductEditorSectionId =
    | "basic"
    | "media"
    | "sku"
    | "effective"
    | "history"

const PRODUCT_EDITOR_SECTIONS: ReadonlyArray<{
    id: ProductEditorSectionId
    label: string
}> = [
    { id: "basic", label: "基础信息" },
    { id: "media", label: "图文信息" },
    { id: "sku", label: "规格与 SKU" },
    { id: "effective", label: "生效信息" },
    { id: "history", label: "历史与引用" },
]

function applySpecsFromDrafts(
    drafts: readonly ProductSpecDraft[],
    current: ProductFields,
    productName = "",
): ProductFields {
    const specs: ProductSpecDimension[] = drafts
        .map((draft) => ({
            name: draft.name.trim(),
            values: draft.values.map((value) => value.trim()).filter(Boolean),
        }))
        .filter((spec) => spec.name)
    const reorderedExisting = current.skus.map((sku) => ({
        ...sku,
        attributeValues: specs.map((spec, nextIndex) => {
            const previousIndex = current.specs.findIndex(
                (previous) => previous.name.trim() === spec.name,
            )
            return (
                sku.attributeValues[
                    previousIndex >= 0 ? previousIndex : nextIndex
                ] ?? ""
            )
        }),
    }))
    const skus = rebuildSkusFromSpecs({
        specs,
        existing: reorderedExisting,
        baseUnit: current.baseUnit,
        skuNoPrefix: "SKU",
        defaultSkuName: productName,
    })
    return { ...current, specs, skus }
}

function validateProductEditor(
    values: ProductEditorFormValues,
    fields: ProductFields,
): string | null {
    if (values.name.trim().length < 2) return "请填写商品名称"
    if (values.changeReason.trim().length < 2) {
        return "请填写本次保存的变更原因"
    }
    return validateProductFields(fields)
}

function productSectionForValidationError(
    message: string,
): ProductEditorSectionId {
    if (message.includes("变更原因") || message.includes("生效")) {
        return "effective"
    }
    if (
        message.includes("SKU") ||
        message.includes("规格") ||
        message.includes("主图") ||
        message.includes("销售价") ||
        message.includes("市场价")
    ) {
        return "sku"
    }
    return "basic"
}

function parseProductSectionId(
    hash: string,
    isCreate: boolean,
): ProductEditorSectionId {
    const raw = hash.replace(/^#/, "")
    const id = raw.startsWith("product-section-")
        ? raw.slice("product-section-".length)
        : raw
    const match = PRODUCT_EDITOR_SECTIONS.find((section) => section.id === id)
    if (!match || (isCreate && match.id === "history")) {
        return "basic"
    }
    return match.id
}

function productDetailToFields(detail: ProductDetailView): ProductFields {
    return {
        lifecycleStatus: detail.lifecycleStatus,
        productNo: detail.productNo,
        description: detail.description ?? "",
        specification: detail.specification ?? "",
        baseUnitId: detail.baseUnitId,
        baseUnitCode: detail.baseUnitCode,
        baseUnit: detail.baseUnit,
        categoryId: detail.categoryId,
        category: detail.category,
        brandId: detail.brandId,
        brand: detail.brand,
        productKind: "",
        carouselImages: [...detail.carouselImages],
        detailImages: [...detail.detailImages],
        carouselPreviewUrls: { ...detail.carouselPreviewUrls },
        detailPreviewUrls: { ...detail.detailPreviewUrls },
        carouselFileAssetIds: { ...detail.carouselFileAssetIds },
        detailFileAssetIds: { ...detail.detailFileAssetIds },
        specs: detail.specs.map((s) => ({
            name: s.name,
            values: [...s.values],
        })),
        skus: detail.skus.map((sku) => ({
            ...sku,
            attributeValues: [...sku.attributeValues],
        })),
    }
}

const EMPTY_BATCH_REFERENCE_PRICE_FIELDS = {
    batchSalePrice: "",
    batchMarketPrice: "",
} as const

function hydrateFromCenter(
    data: MasterDataCenterView,
): ProductEditorFormValues {
    const fields = data.productDetail
        ? productDetailToFields(data.productDetail)
        : emptyProductFields()
    return {
        name: data.name,
        effectiveFrom: data.currentRevision.effectiveFrom,
        effectiveTo: data.currentRevision.effectiveTo ?? "",
        changeReason: "",
        fields: {
            ...fields,
            productKind: data.productKind ?? "",
        },
        specDrafts: fields.specs.map((s) => ({
            name: s.name,
            values: [...s.values],
        })),
        ...EMPTY_BATCH_REFERENCE_PRICE_FIELDS,
    }
}

function createProductDefaults(isCreate: boolean): ProductEditorFormValues {
    return {
        name: "",
        effectiveFrom: defaultImmediateEffectiveFrom(),
        effectiveTo: "",
        changeReason: isCreate ? "新建商品" : "",
        fields: emptyProductFields(),
        specDrafts: [],
        ...EMPTY_BATCH_REFERENCE_PRICE_FIELDS,
    }
}

export {
    applySpecsFromDrafts,
    createProductDefaults,
    hydrateFromCenter,
    newIdempotencyKey,
    parseProductSectionId,
    PRODUCT_EDITOR_SECTIONS,
    productSectionForValidationError,
    validateProductEditor,
}
export type {
    ProductEditorFormValues,
    ProductEditorSectionId,
    ProductSpecDraft,
}
