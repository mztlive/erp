/**
 * 供应商商品编辑表单模型：与 W14 公司商品分区同构，
 * 分类/品牌/单位/规格使用同一套字典与规格维度编辑；
 * 另含供应商独有字段；用于中心页详情即编辑。
 * 入池仅关联已有公司 SKU，不提供「从来源新建公司商品」。
 */

import { BASE_UNIT_DICTIONARY } from "@/features/master-data/resource-fields"
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
  | "supply"
  | "mapping"

export const SUPPLIER_PRODUCT_EDITOR_SECTIONS: ReadonlyArray<{
  id: SupplierProductEditorSectionId
  label: string
  editOnly?: boolean
}> = [
  { id: "basic", label: "基础信息" },
  { id: "media", label: "图文信息" },
  { id: "sku", label: "SKU / 规格" },
  { id: "supply", label: "来源供给" },
  { id: "mapping", label: "映射与入池", editOnly: true },
]

/** 商品能力（多选），与供给/发布侧 capability 编码对齐。 */
export const PRODUCT_CAPABILITY_OPTIONS = [
  { value: "cancel", label: "可取消" },
  { value: "refund", label: "可退款" },
  { value: "logistics", label: "物流配送" },
  { value: "electronic", label: "电子履约" },
  { value: "cold_chain", label: "冷链" },
  { value: "dropship", label: "一件代发" },
] as const

export type ProductCapabilityCode =
  (typeof PRODUCT_CAPABILITY_OPTIONS)[number]["value"]

export type SupplierSpecDraft = Readonly<{
  name: string
  values: readonly string[]
}>

export type SupplierProductFormFields = Readonly<{
  /** 身份与来源（供应商独有） */
  supplierId: string
  supplierName: string
  sourceType: Exclude<SupplierCatalogSourceType, "API"> | "API"
  sourceReference: string
  supplierSpuCode: string
  supplierSkuCode: string

  /** 与公司商品同构的内容字段（字典 ID + 展示快照） */
  name: string
  description: string
  categoryId: string
  category: string
  brandId: string
  brand: string
  baseUnitId: string
  baseUnitCode: string
  baseUnit: string
  barcode: string
  /** 规格维度：与公司商品相同编辑方式 */
  specDrafts: readonly SupplierSpecDraft[]
  carouselImages: readonly string[]
  detailImages: readonly string[]
  skuMainImage: string

  /** 供应商独有：报价与可供条件（非公司商品池价） */
  sourceQuotedPriceGross: string
  inputTaxRate: string
  freightAmount: string
  otherFeeAmount: string
  supplyRegionText: string
  availableQuantity: string
  availabilityStatus: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
  expectedShipTime: string
  afterSalesNote: string
  /** 商品能力多选编码 */
  capabilities: readonly string[]

  changeReason: string
}>

export function emptySupplierProductFormFields(
  partial?: Partial<SupplierProductFormFields>,
): SupplierProductFormFields {
  return {
    supplierId: "",
    supplierName: "",
    sourceType: "MANUAL",
    sourceReference: "",
    supplierSpuCode: "",
    supplierSkuCode: "",
    name: "",
    description: "",
    categoryId: "",
    category: "",
    brandId: "",
    brand: "",
    baseUnitId: "",
    baseUnitCode: "",
    baseUnit: "",
    barcode: "",
    specDrafts: [],
    carouselImages: [],
    detailImages: [],
    skuMainImage: "",
    sourceQuotedPriceGross: "",
    inputTaxRate: "0.13",
    freightAmount: "0.00",
    otherFeeAmount: "0.00",
    supplyRegionText: "全国",
    availableQuantity: "",
    availabilityStatus: "AVAILABLE",
    expectedShipTime: "",
    afterSalesNote: "",
    capabilities: [],
    changeReason: "手工录入供应商商品",
    ...partial,
  }
}

export function mediaFromRevision(
  media: readonly SupplierCatalogMediaView[] | undefined,
): Pick<
  SupplierProductFormFields,
  "carouselImages" | "detailImages" | "skuMainImage"
> {
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

/** 来源 attributes → 规格维度草稿（同名合并取值）。 */
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

export function specDraftsToAttributes(
  drafts: readonly SupplierSpecDraft[],
): SupplierCatalogAttributeView[] {
  return drafts.flatMap((draft) => {
    const name = draft.name.trim()
    if (!name) return []
    return draft.values
      .map((value) => value.trim())
      .filter(Boolean)
      .map((value) => ({ name, value }))
  })
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

export function normalizeCapabilityCodes(
  snapshot: readonly string[] | undefined,
): string[] {
  const known = new Set<string>(
    PRODUCT_CAPABILITY_OPTIONS.map((option) => option.value),
  )
  const labelToCode = new Map<string, string>(
    PRODUCT_CAPABILITY_OPTIONS.map((option) => [option.label, option.value]),
  )
  const next: string[] = []
  for (const raw of snapshot ?? []) {
    const value = raw.trim()
    if (!value) continue
    const code = known.has(value)
      ? value
      : (labelToCode.get(value) ?? value)
    if (!next.includes(code)) next.push(code)
  }
  return next
}

export function hydrateSupplierProductForm(input: {
  supplierId: string
  supplierName: string
  sourceType: SupplierCatalogSourceType
  sourceReference?: string
  supplierSpuCode?: string
  supplierSkuCode: string
  revision: SupplierProductRevisionView
  categoryOptions?: readonly { categoryId: string; categoryName: string }[]
  brandOptions?: readonly { brandId: string; brandName: string }[]
}): SupplierProductFormFields {
  const media = mediaFromRevision(input.revision.media)
  const unit = BASE_UNIT_DICTIONARY.find(
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
  return emptySupplierProductFormFields({
    supplierId: input.supplierId,
    supplierName: input.supplierName,
    sourceType: input.sourceType,
    sourceReference: input.sourceReference ?? "",
    supplierSpuCode: input.supplierSpuCode ?? "",
    supplierSkuCode: input.supplierSkuCode,
    name: input.revision.name,
    description: input.revision.description ?? "",
    categoryId: category?.categoryId ?? "",
    category: category?.categoryName ?? input.revision.category,
    brandId: brand?.brandId ?? "",
    brand: brand?.brandName ?? input.revision.brand ?? "",
    baseUnitId: unit?.id ?? "",
    baseUnitCode: unit?.code ?? "",
    baseUnit: unit?.label ?? input.revision.baseUnit ?? "",
    barcode: input.revision.barcode ?? "",
    specDrafts: attributesToSpecDrafts(input.revision.attributes),
    ...media,
    sourceQuotedPriceGross: input.revision.sourceQuotedPriceGross ?? "",
    inputTaxRate: input.revision.inputTaxRate ?? "0.13",
    freightAmount: input.revision.freightAmount ?? "0.00",
    otherFeeAmount: input.revision.otherFeeAmount ?? "0.00",
    supplyRegionText: input.revision.supplyRegion.join("、") || "全国",
    availableQuantity:
      input.revision.availableQuantity === "—"
        ? ""
        : input.revision.availableQuantity,
    availabilityStatus: input.revision.availabilityStatus,
    expectedShipTime: input.revision.expectedShipTime ?? "",
    afterSalesNote: input.revision.afterSalesNote ?? "",
    capabilities: normalizeCapabilityCodes(input.revision.capabilitySnapshot),
    changeReason: "",
  })
}

export function splitRegionText(value: string): string[] {
  return value
    .split(/[，,、]/)
    .map((item) => item.trim())
    .filter(Boolean)
}

export function formToMediaPayload(
  fields: SupplierProductFormFields,
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
    ...(fields.skuMainImage
      ? [
          {
            usage: "SKU_MAIN" as const,
            fileName: fields.skuMainImage,
            sortOrder: 0,
            fileAssetId: `asset:${fields.skuMainImage}`,
            archiveStatus: "ARCHIVED" as const,
          },
        ]
      : []),
  ]
}

export function validateSupplierProductForm(
  fields: SupplierProductFormFields,
  options?: { requireChangeReason?: boolean; isCreate?: boolean },
): string | null {
  if (!fields.supplierId.trim() && options?.isCreate) {
    return "请选择供应商"
  }
  if (fields.supplierSkuCode.trim().length < 1) {
    return "请填写供应商 SKU 编码"
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
  if (!/^\d+(?:\.\d{1,4})?$/.test(fields.sourceQuotedPriceGross.trim())) {
    return "请输入正确的供应商含税报价，最多 4 位小数"
  }
  if (!/^0(?:\.\d{1,6})?$|^1(?:\.0+)?$/.test(fields.inputTaxRate.trim())) {
    return "请输入 0 到 1 的进项税率"
  }
  if (fields.supplyRegionText.trim().length < 1) {
    return "请填写可供区域"
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
      id: "mainImage",
      label: "SKU 主图（建公司品启用时必填）",
      ok: fields.skuMainImage.trim().length >= 1,
      section: "media" as const,
    },
    {
      id: "quote",
      label: "来源报价",
      ok: /^\d+(?:\.\d{1,4})?$/.test(fields.sourceQuotedPriceGross.trim()),
      section: "supply" as const,
    },
    {
      id: "skuCode",
      label: "供应商 SKU 编码",
      ok: fields.supplierSkuCode.trim().length >= 1,
      section: "basic" as const,
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
