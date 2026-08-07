/**
 * 商品 SPU + 规格 → SKU 组合工具。
 * 规格标识（specification_signature）为系统内部派生，不在 UI 展示或手填。
 */

import type {
  ProductFields,
  ProductSkuFields,
  ProductSpecDimension,
} from "@/features/master-data/types"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"

/** 笛卡尔积生成规格取值组合。无规格时返回单行空组合。 */
export function cartesianSpecValues(
  specs: readonly ProductSpecDimension[]
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
    [[]]
  )
}

export function formatSpecLabel(
  specs: readonly ProductSpecDimension[],
  attributeValues: readonly string[]
): string {
  const active = specs.filter(
    (s) => s.name.trim() && s.values.some((v) => v.trim())
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
 * 数据模型按属性代码 / 属性值代码排序的规范化序列计算，此处以规格名 /
 * 取值文本近似，差距见任务遗留说明。
 */
export function computeSpecificationSignature(
  specs: readonly ProductSpecDimension[],
  attributeValues: readonly string[]
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
}): ProductSkuFields[] {
  const combos = cartesianSpecValues(input.specs)
  const prefix = (input.skuNoPrefix ?? "SKU").replace(/-+$/, "")
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
    const signature = computeSpecificationSignature(input.specs, attributeValues)
    const matched = existingBySignature.get(signature)
    return {
      skuId: matched?.skuId,
      specificationSignature: signature,
      /** 系统默认生成；已有编号或用户覆盖则保留。 */
      skuNo: matched?.skuNo || `${prefix}-${String(index + 1).padStart(2, "0")}`,
      attributeValues: [...attributeValues],
      specLabel: formatSpecLabel(input.specs, attributeValues),
      barcode: matched?.barcode,
      mainImage: matched?.mainImage ?? "",
      mainImagePreviewUrl: matched?.mainImagePreviewUrl,
      mainImageAssetId: matched?.mainImageAssetId,
      salePrice: matched?.salePrice,
      marketPrice: matched?.marketPrice,
      baseUnit: matched?.baseUnit ?? input.baseUnit,
      lifecycleStatus: matched?.lifecycleStatus ?? "ENABLED",
    }
  })
}

export function emptyProductFields(): ProductFields {
  return {
    description: "",
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
        specificationSignature: "",
        attributeValues: [],
        specLabel: "默认规格",
        mainImage: "",
        lifecycleStatus: "ENABLED",
      },
    ],
  }
}

export function productSpecsSummary(
  specs: readonly ProductSpecDimension[]
): string {
  const active = specs.filter(
    (s) => s.name.trim() && s.values.some((v) => v.trim())
  )
  if (active.length === 0) return "无规格（单 SKU）"
  return active
    .map((s) => `${s.name}（${s.values.filter((v) => v.trim()).length}）`)
    .join(" · ")
}

export function joinMediaNames(names: readonly string[] | undefined): string {
  return (names ?? []).filter(Boolean).join(", ")
}

export function splitMediaNames(value: string | undefined): string[] {
  if (!value?.trim()) return []
  return value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
}

/** 列表/概览用的 SPU 关键事实（不含规格标识、不含 SKU 主图）。 */
export function productListFacts(fields: ProductFields): ReadonlyArray<{
  label: string
  value: string
}> {
  const facts: { label: string; value: string }[] = [
    { label: "基础单位", value: fields.baseUnit },
    {
      label: "商品类型",
      value: fields.productKind ? PRODUCT_KIND_LABELS[fields.productKind] : "",
    },
    { label: "分类", value: fields.category },
    { label: "品牌", value: fields.brand },
  ]
  facts.push({
    label: "规格",
    value: productSpecsSummary(fields.specs),
  })
  facts.push({ label: "SKU 数", value: String(fields.skus.length) })
  if (fields.carouselImages.length > 0) {
    facts.push({
      label: "轮播图",
      value: `${fields.carouselImages.length} 张`,
    })
  }
  if (fields.detailImages.length > 0) {
    facts.push({
      label: "详情图",
      value: `${fields.detailImages.length} 张`,
    })
  }
  return facts.filter((f) => f.value)
}

/** 校验：每个启用 SKU 必须有主图；SPU 级字段完整。 */
export function validateProductFields(fields: ProductFields): string | null {
  if (!fields.productKind) return "请选择商品类型"
  if (!fields.baseUnitId.trim() || !fields.baseUnitCode.trim() || !fields.baseUnit.trim()) {
    return "请选择有效的基础单位"
  }
  if (!fields.categoryId.trim() || !fields.category.trim()) return "请选择有效分类"
  if (!fields.brandId.trim() || !fields.brand.trim()) return "请选择有效品牌"
  if (fields.skus.length === 0) return "请至少生成一个 SKU"
  const skuNos = new Set<string>()
  for (const sku of fields.skus) {
    if (!sku.skuNo.trim()) return "SKU 编号不能为空"
    if (skuNos.has(sku.skuNo.trim())) return `SKU 编号「${sku.skuNo.trim()}」重复`
    skuNos.add(sku.skuNo.trim())
    if (sku.lifecycleStatus === "ENABLED" && !sku.mainImage.trim()) {
      return `启用中的 SKU「${sku.skuNo}」必须上传主图`
    }
    const moneyFields: Array<[string, string | undefined]> = [
      ["销售可见价", sku.salePrice],
      ["市场价", sku.marketPrice],
    ]
    try {
      for (const [label, value] of moneyFields) {
        if (!value) continue
        parseDecimal(value, { maxScale: 4 })
        if (compareDecimal(value, "0", 4) < 0) return `${label}不得为负数`
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
