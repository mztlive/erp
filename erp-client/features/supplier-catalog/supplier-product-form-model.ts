/**
 * 供应商商品编辑表单模型：与 W14 公司商品分区同构。
 * - SPU：名称、描述、分类、品牌、单位、轮播/详情图、规格维度
 * - SKU 表：规格组合生成多行，每行可编辑编码/条码/主图/来源供给
 * 来源供给字段属于 supplier_catalog_sku_revision。
 */

import type {
  SupplierCatalogAttributeView,
  SupplierCatalogMediaUsage,
  SupplierCatalogMediaView,
  SupplierCatalogSourceType,
  SupplierProductRevisionView,
} from "@/features/supplier-catalog/types"

export type SupplierProductEditorSectionId =
  | "basic"
  | "media"
  | "sku"
  | "mapping"

export const SUPPLIER_PRODUCT_EDITOR_SECTIONS: ReadonlyArray<{
  id: SupplierProductEditorSectionId
  label: string
  editOnly?: boolean
}> = [
  { id: "basic", label: "基础信息" },
  { id: "media", label: "图文信息" },
  { id: "sku", label: "SKU / 规格与供给" },
  { id: "mapping", label: "映射与入池", editOnly: true },
]

export type SupplierSpecDraft = Readonly<{
  name: string
  values: readonly string[]
}>

/** 一行供应商 SKU：规格取值 + 可编辑身份与来源供给。 */
export type SupplierSkuFormRow = Readonly<{
  /** 稳定行键（会话内）；保存后可对应 catalog sku id */
  rowKey: string
  catalogSkuId?: string
  attributeValues: readonly string[]
  specLabel: string
  supplierSkuCode: string
  barcode: string
  mainImage: string
  /** 一件代发底价（含税运） */
  dropshipFloorPriceGross: string
  /** 集采底价（含税） */
  bulkFloorPriceGross: string
  /** 集采起订量 */
  bulkMinimumOrderQuantity: string
  availableQuantity: string
  availabilityStatus: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
}>

export type SupplierProductFormFields = Readonly<{
  supplierId: string
  supplierName: string
  sourceType: Exclude<SupplierCatalogSourceType, "API"> | "API"
  sourceReference: string
  supplierSpuCode: string

  name: string
  description: string
  categoryId: string
  category: string
  brandId: string
  brand: string
  baseUnitId: string
  baseUnitCode: string
  baseUnit: string
  specDrafts: readonly SupplierSpecDraft[]
  carouselImages: readonly string[]
  detailImages: readonly string[]

  /** 由规格维度组合生成；每行独立来源供给 */
  skus: readonly SupplierSkuFormRow[]

  changeReason: string
}>

export function emptySupplierSkuFormRow(
  partial?: Partial<SupplierSkuFormRow>,
): SupplierSkuFormRow {
  return {
    rowKey: partial?.rowKey ?? newSkuRowKey(),
    catalogSkuId: partial?.catalogSkuId,
    attributeValues: partial?.attributeValues ?? [],
    specLabel: partial?.specLabel ?? "默认规格",
    supplierSkuCode: partial?.supplierSkuCode ?? "",
    barcode: partial?.barcode ?? "",
    mainImage: partial?.mainImage ?? "",
    dropshipFloorPriceGross: partial?.dropshipFloorPriceGross ?? "",
    bulkFloorPriceGross: partial?.bulkFloorPriceGross ?? "",
    bulkMinimumOrderQuantity: partial?.bulkMinimumOrderQuantity ?? "1",
    availableQuantity: partial?.availableQuantity ?? "",
    availabilityStatus: partial?.availabilityStatus ?? "AVAILABLE",
  }
}

function newSkuRowKey(): string {
  return `sku-row-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`
}

export function emptySupplierProductFormFields(
  partial?: Partial<SupplierProductFormFields>,
): SupplierProductFormFields {
  return {
    supplierId: "",
    supplierName: "",
    sourceType: "MANUAL",
    sourceReference: "",
    supplierSpuCode: "",
    name: "",
    description: "",
    categoryId: "",
    category: "",
    brandId: "",
    brand: "",
    baseUnitId: "",
    baseUnitCode: "",
    baseUnit: "",
    specDrafts: [],
    carouselImages: [],
    detailImages: [],
    skus: [emptySupplierSkuFormRow({ supplierSkuCode: "SKU-01" })],
    changeReason: "手工录入供应商商品",
    ...partial,
  }
}

/** 笛卡尔积生成规格取值组合。无规格时返回单行空组合。 */
export function cartesianSpecValues(
  specs: readonly SupplierSpecDraft[],
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

export function formatSpecLabel(
  specs: readonly SupplierSpecDraft[],
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

function sameAttributeValues(
  a: readonly string[],
  b: readonly string[],
): boolean {
  if (a.length !== b.length) return false
  return a.every((v, i) => v === b[i])
}

/** 按当前规格维度重建 SKU 行；尽量按属性取值匹配保留编码/主图/供给。 */
export function rebuildSupplierSkusFromSpecs(input: {
  specs: readonly SupplierSpecDraft[]
  existing: readonly SupplierSkuFormRow[]
  skuCodePrefix?: string
}): SupplierSkuFormRow[] {
  const combos = cartesianSpecValues(input.specs)
  const prefix = (input.skuCodePrefix ?? "SKU").replace(/-+$/, "")

  return combos.map((attributeValues, index) => {
    const specLabel = formatSpecLabel(input.specs, attributeValues)
    const matched =
      input.existing.find((sku) =>
        sameAttributeValues(sku.attributeValues, attributeValues),
      ) ??
      (combos.length === 1 && input.existing.length === 1
        ? input.existing[0]
        : undefined)

    return emptySupplierSkuFormRow({
      ...matched,
      rowKey: matched?.rowKey ?? newSkuRowKey(),
      attributeValues: [...attributeValues],
      specLabel,
      supplierSkuCode:
        matched?.supplierSkuCode ||
        `${prefix}-${String(index + 1).padStart(2, "0")}`,
    })
  })
}

export function mediaFromRevision(
  media: readonly SupplierCatalogMediaView[] | undefined,
): {
  carouselImages: string[]
  detailImages: string[]
  skuMainImage: string
} {
  const list = media ?? []
  const names = (usage: SupplierCatalogMediaUsage) =>
    list
      .filter((entry) => entry.usage === usage)
      .sort((a, b) => a.sortOrder - b.sortOrder)
      .map((entry) => entry.fileName)
  return {
    carouselImages: names("SPU_CAROUSEL"),
    detailImages: names("SPU_DETAIL"),
    skuMainImage: names("SKU_MAIN")[0] ?? "",
  }
}

export function attributesToSpecDrafts(
  attributes: readonly SupplierCatalogAttributeView[] | undefined,
): SupplierSpecDraft[] {
  const map = new Map<string, string[]>()
  for (const attribute of attributes ?? []) {
    const name = attribute.name.trim()
    const value = attribute.value.trim()
    if (!name || !value) continue
    const existing = map.get(name) ?? []
    if (!existing.includes(value)) existing.push(value)
    map.set(name, existing)
  }
  return [...map.entries()].map(([name, values]) => ({ name, values }))
}

/** 多 SKU 时 attributes 合并各行首取值 + 维度名；单 SKU 写全量。 */
export function skusToAttributes(
  skus: readonly SupplierSkuFormRow[],
  specs: readonly SupplierSpecDraft[],
): SupplierCatalogAttributeView[] {
  const active = specs.filter(
    (s) => s.name.trim() && s.values.some((v) => v.trim()),
  )
  if (active.length === 0) return []
  if (skus.length === 1) {
    return active
      .map((spec, index) => ({
        name: spec.name.trim(),
        value: (skus[0]?.attributeValues[index] ?? "").trim(),
      }))
      .filter((item) => item.name && item.value)
  }
  // 多 SKU：规格维度取值集中到 attributes（各维全部取值）
  return active.flatMap((spec) =>
    spec.values
      .map((v) => v.trim())
      .filter(Boolean)
      .map((value) => ({ name: spec.name.trim(), value })),
  )
}

export function deriveSpecification(
  drafts: readonly SupplierSpecDraft[],
  fallback = "默认规格",
): string {
  const active = drafts.filter(
    (draft) =>
      draft.name.trim() && draft.values.some((value) => value.trim()),
  )
  if (active.length === 0) return fallback
  return active
    .map((draft) => {
      const values = draft.values.map((value) => value.trim()).filter(Boolean)
      return `${draft.name.trim()}：${values.join("/")}`
    })
    .join(" / ")
}

export type CatalogSkuSeed = Readonly<{
  id?: string
  supplierSkuCode: string
  revision: SupplierProductRevisionView
}>

export function hydrateSupplierProductForm(input: {
  supplierId: string
  supplierName: string
  sourceType: SupplierCatalogSourceType
  sourceReference?: string
  supplierSpuCode?: string
  supplierSkuCode: string
  revision: SupplierProductRevisionView
  /** 多 SKU 时优先使用；否则退化为单条 currentRevision */
  catalogSkus?: readonly CatalogSkuSeed[]
  categoryOptions?: readonly { categoryId: string; categoryName: string }[]
  brandOptions?: readonly { brandId: string; brandName: string }[]
  /** 计量单位选项（来自 /admin/unit-of-measures）；缺省时按名称/代码匹配 */
  unitOptions?: readonly { id: string; code: string; label: string }[]
}): SupplierProductFormFields {
  const media = mediaFromRevision(input.revision.media)
  const unit = input.unitOptions?.find(
    (candidate) =>
      candidate.label === input.revision.baseUnit ||
      candidate.code === input.revision.baseUnit,
  )
  const category = input.categoryOptions?.find(
    (candidate) => candidate.categoryName === input.revision.category,
  )
  const brand = input.brandOptions?.find(
    (candidate) => candidate.brandName === input.revision.brand,
  )

  const specDrafts =
    input.catalogSkus && input.catalogSkus.length > 1
      ? attributesToSpecDrafts(
          input.catalogSkus.flatMap((entry) => entry.revision.attributes ?? []),
        )
      : attributesToSpecDrafts(input.revision.attributes)

  const skuRows: SupplierSkuFormRow[] =
    input.catalogSkus && input.catalogSkus.length > 0
      ? input.catalogSkus.map((entry, index) => {
          const attrs = entry.revision.attributes ?? []
          const attributeValues = specDrafts.map((dim) => {
            const hit = attrs.find((a) => a.name.trim() === dim.name.trim())
            return hit?.value.trim() ?? ""
          })
          const main =
            mediaFromRevision(entry.revision.media).skuMainImage ||
            (index === 0 ? media.skuMainImage : "")
          return emptySupplierSkuFormRow({
            catalogSkuId: entry.id,
            attributeValues,
            specLabel: formatSpecLabel(specDrafts, attributeValues),
            supplierSkuCode: entry.supplierSkuCode,
            barcode: entry.revision.barcode ?? "",
            mainImage: main,
            dropshipFloorPriceGross:
              entry.revision.dropshipFloorPriceGross ?? "",
            bulkFloorPriceGross: entry.revision.bulkFloorPriceGross ?? "",
            bulkMinimumOrderQuantity:
              entry.revision.bulkMinimumOrderQuantity ?? "1",
            availableQuantity:
              entry.revision.availableQuantity === "—"
                ? ""
                : entry.revision.availableQuantity,
            availabilityStatus: entry.revision.availabilityStatus,
          })
        })
      : [
          emptySupplierSkuFormRow({
            attributeValues: specDrafts.map((dim) => {
              const hit = (input.revision.attributes ?? []).find(
                (a) => a.name.trim() === dim.name.trim(),
              )
              return hit?.value.trim() ?? (dim.values[0] ?? "")
            }),
            specLabel: formatSpecLabel(
              specDrafts,
              specDrafts.map((dim) => {
                const hit = (input.revision.attributes ?? []).find(
                  (a) => a.name.trim() === dim.name.trim(),
                )
                return hit?.value.trim() ?? (dim.values[0] ?? "")
              }),
            ),
            supplierSkuCode: input.supplierSkuCode,
            barcode: input.revision.barcode ?? "",
            mainImage: media.skuMainImage,
            dropshipFloorPriceGross:
              input.revision.dropshipFloorPriceGross ?? "",
            bulkFloorPriceGross: input.revision.bulkFloorPriceGross ?? "",
            bulkMinimumOrderQuantity:
              input.revision.bulkMinimumOrderQuantity ?? "1",
            availableQuantity:
              input.revision.availableQuantity === "—"
                ? ""
                : input.revision.availableQuantity,
            availabilityStatus: input.revision.availabilityStatus,
          }),
        ]

  // 规格变更后与笛卡尔积对齐（保留已匹配行）
  const skus = rebuildSupplierSkusFromSpecs({
    specs: specDrafts,
    existing: skuRows,
    skuCodePrefix: input.supplierSkuCode.replace(/-\d+$/, "") || "SKU",
  })

  return emptySupplierProductFormFields({
    supplierId: input.supplierId,
    supplierName: input.supplierName,
    sourceType: input.sourceType,
    sourceReference: input.sourceReference ?? "",
    supplierSpuCode: input.supplierSpuCode ?? "",
    name: input.revision.name,
    description: input.revision.description ?? "",
    categoryId: category?.categoryId ?? "",
    category: category?.categoryName ?? input.revision.category,
    brandId: brand?.brandId ?? "",
    brand: brand?.brandName ?? input.revision.brand ?? "",
    baseUnitId: unit?.id ?? "",
    baseUnitCode: unit?.code ?? "",
    baseUnit: unit?.label ?? input.revision.baseUnit ?? "",
    specDrafts,
    carouselImages: media.carouselImages,
    detailImages: media.detailImages,
    skus,
    changeReason: "",
  })
}

export function formToMediaPayload(
  fields: SupplierProductFormFields,
  sku: SupplierSkuFormRow,
): readonly Omit<SupplierCatalogMediaView, "id">[] {
  return [
    ...fields.carouselImages.map((fileName, index) => ({
      usage: "SPU_CAROUSEL" as const,
      fileName,
      sortOrder: index,
      fileAssetId: `asset:${fileName}`,
      archiveStatus: "ARCHIVED" as const,
    })),
    ...fields.detailImages.map((fileName, index) => ({
      usage: "SPU_DETAIL" as const,
      fileName,
      sortOrder: index,
      fileAssetId: `asset:${fileName}`,
      archiveStatus: "ARCHIVED" as const,
    })),
    ...(sku.mainImage
      ? [
          {
            usage: "SKU_MAIN" as const,
            fileName: sku.mainImage,
            sortOrder: 0,
            fileAssetId: `asset:${sku.mainImage}`,
            archiveStatus: "ARCHIVED" as const,
          },
        ]
      : []),
  ]
}

export function skuRowToPayload(sku: SupplierSkuFormRow) {
  return {
    supplierSkuCode: sku.supplierSkuCode.trim(),
    barcode: sku.barcode.trim() || undefined,
    mainImage: sku.mainImage.trim() || undefined,
    specification: sku.specLabel,
    attributes: sku.attributeValues
      .map((value) => ({
        name: "", // filled by caller with dim names
        value: value.trim(),
      }))
      .filter((item) => item.value),
    dropshipFloorPriceGross: sku.dropshipFloorPriceGross.trim(),
    bulkFloorPriceGross: sku.bulkFloorPriceGross.trim(),
    bulkMinimumOrderQuantity: sku.bulkMinimumOrderQuantity.trim(),
    availableQuantity: sku.availableQuantity.trim() || undefined,
    availabilityStatus: sku.availabilityStatus,
  }
}

export function validateSupplierProductForm(
  fields: SupplierProductFormFields,
  options?: { requireChangeReason?: boolean; isCreate?: boolean },
): string | null {
  if (!fields.supplierId.trim() && options?.isCreate) {
    return "请选择供应商"
  }
  if (fields.name.trim().length < 2) return "请填写商品名称"
  if (!fields.categoryId.trim() && !fields.category.trim()) {
    return "请选择分类"
  }
  if (!fields.brandId.trim() && !fields.brand.trim()) {
    return "请选择品牌"
  }
  if (!fields.baseUnitId.trim() && !fields.baseUnit.trim()) {
    return "请选择基础单位"
  }
  if (fields.skus.length < 1) return "至少保留一个 SKU"
  const codes = new Set<string>()
  for (const [index, sku] of fields.skus.entries()) {
    const label = sku.specLabel || `第 ${index + 1} 行`
    if (sku.supplierSkuCode.trim().length < 1) {
      return `${label}：请填写供应商 SKU 编码`
    }
    const code = sku.supplierSkuCode.trim()
    if (codes.has(code)) {
      return `供应商 SKU 编码重复：${code}`
    }
    codes.add(code)
    if (!/^\d+(?:\.\d{1,4})?$/.test(sku.dropshipFloorPriceGross.trim())) {
      return `${label}：请输入正确的一件代发底价（含税运），最多 4 位小数`
    }
    if (!/^\d+(?:\.\d{1,4})?$/.test(sku.bulkFloorPriceGross.trim())) {
      return `${label}：请输入正确的集采底价（含税），最多 4 位小数`
    }
    if (!/^\d+(?:\.\d{1,6})?$/.test(sku.bulkMinimumOrderQuantity.trim())) {
      return `${label}：请输入正确的集采起订量`
    }
  }
  if (options?.requireChangeReason && fields.changeReason.trim().length < 2) {
    return "请填写本次保存的变更原因"
  }
  return null
}

export function supplierProductCompleteness(
  fields: SupplierProductFormFields,
): {
  checks: readonly {
    id: string
    label: string
    ok: boolean
    section: SupplierProductEditorSectionId
  }[]
  completed: number
  total: number
  percent: number
} {
  const allSkuPricesOk = fields.skus.every(
    (sku) =>
      /^\d+(?:\.\d{1,4})?$/.test(sku.dropshipFloorPriceGross.trim()) &&
      /^\d+(?:\.\d{1,4})?$/.test(sku.bulkFloorPriceGross.trim()) &&
      /^\d+(?:\.\d{1,6})?$/.test(sku.bulkMinimumOrderQuantity.trim()),
  )
  const allSkuCodesOk = fields.skus.every(
    (sku) => sku.supplierSkuCode.trim().length >= 1,
  )
  const checks = [
    {
      id: "name",
      label: "商品名称",
      ok: fields.name.trim().length >= 2,
      section: "basic" as const,
    },
    {
      id: "category",
      label: "分类",
      ok: Boolean(fields.categoryId.trim() || fields.category.trim()),
      section: "basic" as const,
    },
    {
      id: "brand",
      label: "品牌",
      ok: Boolean(fields.brandId.trim() || fields.brand.trim()),
      section: "basic" as const,
    },
    {
      id: "unit",
      label: "基础单位",
      ok: Boolean(fields.baseUnitId.trim() || fields.baseUnit.trim()),
      section: "basic" as const,
    },
    {
      id: "skuCode",
      label: "全部 SKU 编码",
      ok: fields.skus.length > 0 && allSkuCodesOk,
      section: "sku" as const,
    },
    {
      id: "prices",
      label: "全部 SKU 代发/集采底价与起订量",
      ok: fields.skus.length > 0 && allSkuPricesOk,
      section: "sku" as const,
    },
  ] as const
  const completed = checks.filter((item) => item.ok).length
  return {
    checks,
    completed,
    total: checks.length,
    percent: Math.round((completed / checks.length) * 100),
  }
}
