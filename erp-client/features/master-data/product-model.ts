/**
 * 商品 SPU + 规格 → SKU 组合工具。
 * 规格标识（specification_signature）为系统内部派生，不在 UI 展示或手填。
 */

import type {
    ProductFields,
    ProductSkuFields,
    ProductSpecDimension,
} from "@/features/master-data/types"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"

/** 笛卡尔积生成规格取值组合。无规格时返回单行空组合。 */
function cartesianSpecValues(
    specs: readonly ProductSpecDimension[],
): readonly (readonly string[])[] {
    const active = specs
        .map((s) => ({
            name: s.name.trim(),
            values: s.values.map((v) => v.trim()).filter(Boolean),
        }))
        .filter((s) => s.name && s.values.length > 0)

    if (active.length === 0) return [[]]

    return active.reduce<readonly (readonly string[])[]>(
        (acc, dim) => {
            const next: string[][] = []
            for (const prefix of acc) {
                for (const value of dim.values) {
                    next.push([...prefix, value])
                }
            }
            return next
        },
        [[]],
    )
}

function formatSpecLabel(
    specs: readonly ProductSpecDimension[],
    attributeValues: readonly string[],
): string {
    const active = specs.filter(
        (s) => s.name.trim() && s.values.some((v) => v.trim()),
    )
    if (active.length === 0 || attributeValues.length === 0) {
        return "默认规格"
    }
    return active
        .map((s, i) => `${s.name.trim()}：${attributeValues[i] ?? ""}`)
        .join(" / ")
}

/**
 * 计算规范化规格签名（`specification_signature`）。
 * 取有值规格的「规格名=取值」对，按规格名排序后拼接，与输入顺序无关；
 * 无规格时返回固定空签名（同一 SPU 最多一个无规格 SKU）。
 * 前后端均以 SPU 局部规格名 / 取值文本计算，不要求预先维护全局规格字典。
 */
function computeSpecificationSignature(
    specs: readonly ProductSpecDimension[],
    attributeValues: readonly string[],
): string {
    const active = specs
        .map((spec, index) => ({
            name: spec.name.trim(),
            value: (attributeValues[index] ?? "").trim(),
            values: spec.values.map((v) => v.trim()).filter(Boolean),
        }))
        .filter((s) => s.name && s.values.length > 0)
    if (active.length === 0) return ""
    return active
        .filter((s) => s.value)
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((s) => `${s.name}=${s.value}`)
        .join("|")
}

/** 按当前规格维度重建 SKU 行；仅签名一致时复用旧行，否则新建。 */
export function rebuildSkusFromSpecs(input: {
    specs: readonly ProductSpecDimension[]
    existing: readonly ProductSkuFields[]
    baseUnit: string
    skuNoPrefix?: string
    /** 新建 SKU 行时的默认名称（通常取商品名称）。 */
    defaultSkuName?: string
}): ProductSkuFields[] {
    const combos = cartesianSpecValues(input.specs)
    const prefix = (input.skuNoPrefix ?? "SKU").replace(/-+$/, "")
    const defaultSkuName = (input.defaultSkuName ?? "").trim()
    const existingBySignature = new Map<string, ProductSkuFields>()
    for (const sku of input.existing) {
        const signature =
            sku.specificationSignature ??
            computeSpecificationSignature(input.specs, sku.attributeValues)
        if (!existingBySignature.has(signature)) {
            existingBySignature.set(signature, sku)
        }
    }

    return combos.map((attributeValues, index) => {
        const signature = computeSpecificationSignature(
            input.specs,
            attributeValues,
        )
        const matched = existingBySignature.get(signature)
        return {
            skuId: matched?.skuId,
            skuRevisionId: matched?.skuRevisionId,
            requiresExplicitReenable: matched?.requiresExplicitReenable,
            specificationSignature: signature,
            /** 系统默认生成；已有编号或用户覆盖则保留。 */
            skuNo:
                matched?.skuNo ||
                `${prefix}-${String(index + 1).padStart(2, "0")}`,
            /** 已有名称优先；新建行默认带入商品名称。 */
            name: matched?.name?.trim() || defaultSkuName,
            attributeValues: [...attributeValues],
            specLabel: formatSpecLabel(input.specs, attributeValues),
            barcode: matched?.barcode,
            mainImage: matched?.mainImage ?? "",
            mainImagePreviewUrl: matched?.mainImagePreviewUrl,
            mainImageAssetId: matched?.mainImageAssetId,
            salePrice: matched?.salePrice,
            marketPrice: matched?.marketPrice,
            baseUnit: matched?.baseUnit ?? input.baseUnit,
            listingStatus: matched?.listingStatus ?? "UNLISTED",
            lifecycleStatus: matched?.lifecycleStatus ?? "ENABLED",
        }
    })
}

export function emptyProductFields(): ProductFields {
    return {
        lifecycleStatus: "ENABLED",
        productNo: "",
        description: "",
        specification: "",
        baseUnitId: "",
        baseUnitCode: "",
        baseUnit: "",
        categoryId: "",
        category: "",
        brandId: "",
        brand: "",
        productKind: "",
        carouselImages: [],
        detailImages: [],
        carouselPreviewUrls: {},
        detailPreviewUrls: {},
        carouselFileAssetIds: {},
        detailFileAssetIds: {},
        specs: [],
        skus: [
            {
                skuNo: "SKU-01",
                name: "",
                specificationSignature: "",
                attributeValues: [],
                specLabel: "默认规格",
                mainImage: "",
                listingStatus: "UNLISTED",
                lifecycleStatus: "ENABLED",
            },
        ],
    }
}

/** 校验：每个启用 SKU 必须有主图；SPU 级字段完整。 */
export function validateProductFields(fields: ProductFields): string | null {
    if (!fields.productNo.trim()) return "请填写商品编号"
    if (!fields.productKind) return "请选择商品类型"
    if (
        !fields.baseUnitId.trim() ||
        !fields.baseUnitCode.trim() ||
        !fields.baseUnit.trim()
    ) {
        return "请选择有效的基础单位"
    }
    if (!fields.categoryId.trim() || !fields.category.trim())
        return "请选择有效分类"
    if (!fields.brandId.trim() || !fields.brand.trim()) return "请选择有效品牌"
    if (fields.skus.length === 0) return "请至少生成一个 SKU"
    const skuNos = new Set<string>()
    for (const sku of fields.skus) {
        if (!sku.skuNo.trim()) return "SKU 编号不能为空"
        if (!sku.name.trim()) return `SKU「${sku.skuNo.trim() || "未编号"}」名称不能为空`
        if (skuNos.has(sku.skuNo.trim()))
            return `SKU 编号「${sku.skuNo.trim()}」重复`
        skuNos.add(sku.skuNo.trim())
        if (sku.lifecycleStatus === "ENABLED" && !sku.mainImage.trim()) {
            return `启用中的 SKU「${sku.skuNo}」必须上传主图`
        }
        const moneyFields: Array<[string, string | undefined]> = [
            ["销售价", sku.salePrice],
            ["市场价", sku.marketPrice],
        ]
        try {
            for (const [label, value] of moneyFields) {
                if (!value) continue
                parseDecimal(value, { maxScale: 4 })
                if (compareDecimal(value, "0", 4) < 0)
                    return `${label}不得为负数`
            }
        } catch {
            return `SKU「${sku.skuNo}」的参考价格格式不正确`
        }
    }
    const names = new Set<string>()
    for (const dim of fields.specs) {
        const n = dim.name.trim()
        if (!n) continue
        if (names.has(n)) return `规格名称「${n}」重复`
        names.add(n)
        if (dim.values.filter((v) => v.trim()).length === 0) {
            return `规格「${n}」至少填写一个取值`
        }
    }
    return null
}
