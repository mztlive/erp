/**
 * 商品 SPU + 规格 → SKU 组合工具。
 * 规格标识（specification_signature）为系统内部派生，不在 UI 展示或手填。
 */

import type {
  ProductFields,
  ProductSkuFields,
  ProductSpecDimension,
} from "@/features/master-data/types"

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

/** 按当前规格维度重建 SKU 行；尽量按属性取值匹配保留已有主图/价格等。 */
export function rebuildSkusFromSpecs(input: {
  specs: readonly ProductSpecDimension[]
  existing: readonly ProductSkuFields[]
  baseUnit: string
  supplier?: string
  skuNoPrefix?: string
}): ProductSkuFields[] {
  const combos = cartesianSpecValues(input.specs)
  const prefix = (input.skuNoPrefix ?? "SKU").replace(/-+$/, "")

  return combos.map((attributeValues, index) => {
    const specLabel = formatSpecLabel(input.specs, attributeValues)
    const matched =
      input.existing.find((sku) =>
        sameAttributeValues(sku.attributeValues, attributeValues)
      ) ??
      (combos.length === 1 && input.existing.length === 1
        ? input.existing[0]
        : undefined)

    return {
      skuId: matched?.skuId,
      skuNo: matched?.skuNo || `${prefix}-${String(index + 1).padStart(2, "0")}`,
      attributeValues: [...attributeValues],
      specLabel,
      barcode: matched?.barcode,
      mainImage: matched?.mainImage ?? "",
      costPrice: matched?.costPrice,
      salePrice: matched?.salePrice,
      supplier: matched?.supplier ?? input.supplier,
      baseUnit: matched?.baseUnit ?? input.baseUnit,
      lifecycleStatus: matched?.lifecycleStatus ?? "ENABLED",
    }
  })
}

function sameAttributeValues(
  a: readonly string[],
  b: readonly string[]
): boolean {
  if (a.length !== b.length) return false
  return a.every((v, i) => v === b[i])
}

export function emptyProductFields(): ProductFields {
  return {
    baseUnit: "",
    category: "",
    brand: "",
    supplier: undefined,
    carouselImages: [],
    detailImages: [],
    specs: [],
    skus: [
      {
        skuNo: "SKU-01",
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
    { label: "分类", value: fields.category },
    { label: "品牌", value: fields.brand },
  ]
  if (fields.supplier) facts.push({ label: "供应商", value: fields.supplier })
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
  if (!fields.baseUnit.trim()) return "请选择基础单位"
  if (!fields.category.trim()) return "请选择分类"
  if (!fields.brand.trim()) return "请选择品牌"
  if (fields.skus.length === 0) return "请至少生成一个 SKU"
  for (const sku of fields.skus) {
    if (!sku.skuNo.trim()) return "SKU 编号不能为空"
    if (sku.lifecycleStatus === "ENABLED" && !sku.mainImage.trim()) {
      return `启用中的 SKU「${sku.skuNo}」必须上传主图`
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
