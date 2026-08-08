import { readSheet } from "read-excel-file/browser"

import { PRODUCT_KIND_VALUES, type ProductKind } from "@/features/master-data/types"
import type {
  SupplierCatalogAttributeView,
  SupplierCatalogMediaView,
} from "@/features/supplier-catalog/types"

type Cell = string | number | boolean | Date | null

export type ParsedExcelSkuRow = Readonly<{
  rowNo: number
  supplierSkuCode: string
  name: string
  specification: string
  sourceBaseUnit?: string
  barcode?: string
  attributes: readonly SupplierCatalogAttributeView[]
  mainImageUrl?: string
  dropshipFloorPriceGross?: string
  bulkFloorPriceGross?: string
  bulkMinimumOrderQuantity?: string
  availableQuantity?: string
  availabilityStatus: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
}>

export type ParsedExcelProduct = Readonly<{
  supplierSpuCode: string
  name: string
  description?: string
  sourceProductKind?: ProductKind
  sourceCategory?: string
  sourceBrand?: string
  attributes: readonly SupplierCatalogAttributeView[]
  media: readonly Omit<SupplierCatalogMediaView, "id">[]
  sourceRevisionToken?: string
  validFrom?: string
  validTo?: string
  skus: readonly ParsedExcelSkuRow[]
}>

export type ParsedExcelRejectedRow = Readonly<{
  rowNo: number
  supplierSkuCode: string
  errorText: string
}>

export type SupplierCatalogExcelPreview = Readonly<{
  fileName: string
  totalRows: number
  products: readonly ParsedExcelProduct[]
  rejectedRows: readonly ParsedExcelRejectedRow[]
}>

export type SupplierCatalogExcelImportInput = Readonly<{
  supplierId: string
  fileAssetId: string
  sourceReference: string
  preview: SupplierCatalogExcelPreview
  idempotencyKey: string
}>

export type SupplierCatalogExcelImportResult = Readonly<{
  intakeBatchId: string
  productIds: readonly string[]
  importedCount: number
  rejectedCount: number
  replayed: boolean
  reference: string
  recordedAt: string
}>

const HEADER_ALIASES = {
  supplierSpuCode: ["供应商SPU编码", "supplier_spu_code", "spu_code"],
  supplierSkuCode: ["供应商SKU编码", "supplier_sku_code", "sku_code"],
  name: ["商品名称", "供应商商品名称", "name"],
  description: ["商品描述", "description"],
  sourceProductKind: ["商品类型", "source_product_kind", "product_kind"],
  sourceCategory: ["来源分类", "分类", "source_category", "category"],
  sourceBrand: ["来源品牌", "品牌", "source_brand", "brand"],
  specification: ["规格", "specification"],
  sourceBaseUnit: ["来源单位", "单位", "source_base_unit", "unit"],
  barcode: ["条码", "barcode"],
  attributes: ["规格属性", "structured_attributes", "attributes"],
  carouselUrls: ["轮播图URL", "carousel_urls"],
  detailUrls: ["详情图URL", "detail_urls"],
  mainImageUrl: ["SKU主图URL", "主图URL", "main_image_url"],
  dropshipFloorPriceGross: [
    "一件代发底价（含税运）",
    "一件代发底价",
    "dropship_floor_price_gross",
  ],
  bulkFloorPriceGross: ["集采底价（含税）", "集采底价", "bulk_floor_price_gross"],
  bulkMinimumOrderQuantity: ["集采起订量", "bulk_minimum_order_quantity"],
  availableQuantity: ["可供数量", "available_quantity"],
  availabilityStatus: ["可供状态", "availability_status"],
  sourceRevisionToken: ["来源版本", "source_revision_token"],
  validFrom: ["有效期开始", "valid_from"],
  validTo: ["有效期结束", "valid_to"],
} as const

type HeaderKey = keyof typeof HEADER_ALIASES

function normalizedHeader(value: string): string {
  return value.trim().toLowerCase().replace(/[\s_-]+/g, "")
}

function cellText(value: Cell | undefined): string {
  if (value == null) return ""
  if (value instanceof Date) return value.toISOString().slice(0, 10)
  return String(value).trim()
}

function parseCsv(text: string): Cell[][] {
  const rows: string[][] = []
  let row: string[] = []
  let cell = ""
  let quoted = false
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index]
    if (character === '"') {
      if (quoted && text[index + 1] === '"') {
        cell += '"'
        index += 1
      } else {
        quoted = !quoted
      }
    } else if (character === "," && !quoted) {
      row.push(cell)
      cell = ""
    } else if ((character === "\n" || character === "\r") && !quoted) {
      if (character === "\r" && text[index + 1] === "\n") index += 1
      row.push(cell)
      if (row.some((value) => value.trim())) rows.push(row)
      row = []
      cell = ""
    } else {
      cell += character
    }
  }
  row.push(cell)
  if (row.some((value) => value.trim())) rows.push(row)
  return rows
}

async function readRows(file: File): Promise<Cell[][]> {
  if (file.name.toLowerCase().endsWith(".csv")) {
    const text = await file.text()
    return parseCsv(text.replace(/^\uFEFF/, ""))
  }
  return (await readSheet(file)) as unknown as Cell[][]
}

function splitList(value: string): string[] {
  return value
    .split(/[，,、;；\n]/)
    .map((part) => part.trim())
    .filter(Boolean)
}

function parseAttributes(value: string): SupplierCatalogAttributeView[] {
  return splitList(value)
    .map((part) => {
      const [name, ...rest] = part.split(/[:：]/)
      return { name: name?.trim() ?? "", value: rest.join(":").trim() }
    })
    .filter((attribute) => attribute.name && attribute.value)
}

function mediaFromUrls(
  carouselUrls: string,
  detailUrls: string
): Omit<SupplierCatalogMediaView, "id">[] {
  return [
    ...splitList(carouselUrls).map((sourceUrl, index) => ({
      usage: "SPU_CAROUSEL" as const,
      fileName: sourceUrl.split("/").pop() || `carousel-${index + 1}`,
      sortOrder: index,
      sourceUrl,
      archiveStatus: "PENDING_IMPORT" as const,
    })),
    ...splitList(detailUrls).map((sourceUrl, index) => ({
      usage: "SPU_DETAIL" as const,
      fileName: sourceUrl.split("/").pop() || `detail-${index + 1}`,
      sortOrder: index,
      sourceUrl,
      archiveStatus: "PENDING_IMPORT" as const,
    })),
  ]
}

function isMoney(value: string): boolean {
  return !value || /^\d+(?:\.\d{1,4})?$/.test(value)
}

function isQuantity(value: string): boolean {
  return !value || /^\d+(?:\.\d{1,6})?$/.test(value)
}

function isDate(value: string): boolean {
  return !value || /^\d{4}-\d{2}-\d{2}$/.test(value)
}

/**
 * 解析并预检供应商商品 Excel/CSV；不执行任何服务端写入。
 * 必填列为供应商 SPU 编码、供应商 SKU 编码、商品名称和规格。
 */
export async function parseSupplierCatalogExcel(
  file: File
): Promise<SupplierCatalogExcelPreview> {
  const rows = await readRows(file)
  if (rows.length === 0) throw new Error("文件没有可读取的数据")

  const headerIndex = new Map<string, number>()
  rows[0]?.forEach((cell, index) => {
    headerIndex.set(normalizedHeader(cellText(cell)), index)
  })
  const columns = new Map<HeaderKey, number>()
  for (const [key, aliases] of Object.entries(HEADER_ALIASES) as Array<
    [HeaderKey, readonly string[]]
  >) {
    const hit = aliases
      .map(normalizedHeader)
      .map((alias) => headerIndex.get(alias))
      .find((index) => index !== undefined)
    if (hit !== undefined) columns.set(key, hit)
  }
  const missingHeaders = ([
    "supplierSpuCode",
    "supplierSkuCode",
    "name",
    "specification",
  ] as HeaderKey[]).filter((key) => !columns.has(key))
  if (missingHeaders.length > 0) {
    throw new Error(
      `缺少必填列：${missingHeaders
        .map((key) => HEADER_ALIASES[key][0])
        .join("、")}`
    )
  }

  const value = (row: Cell[], key: HeaderKey) =>
    cellText(row[columns.get(key) ?? -1])
  const products = new Map<string, ParsedExcelProduct>()
  const rejectedRows: ParsedExcelRejectedRow[] = []
  const supplierSkuCodes = new Set<string>()
  let totalRows = 0

  rows.slice(1).forEach((row, dataIndex) => {
    if (!row.some((cell) => cellText(cell))) return
    totalRows += 1
    const rowNo = dataIndex + 2
    const supplierSpuCode = value(row, "supplierSpuCode")
    const supplierSkuCode = value(row, "supplierSkuCode")
    const name = value(row, "name")
    const specification = value(row, "specification")
    const productKind = value(row, "sourceProductKind")
    const availabilityStatus = value(row, "availabilityStatus").toUpperCase() || "AVAILABLE"
    const dropship = value(row, "dropshipFloorPriceGross")
    const bulk = value(row, "bulkFloorPriceGross")
    const moq = value(row, "bulkMinimumOrderQuantity")
    const available = value(row, "availableQuantity")
    const validFrom = value(row, "validFrom")
    const validTo = value(row, "validTo")
    const errors: string[] = []
    if (!supplierSpuCode) errors.push("供应商 SPU 编码为空")
    if (!supplierSkuCode) errors.push("供应商 SKU 编码为空")
    if (!name) errors.push("商品名称为空")
    if (!specification) errors.push("规格为空")
    if (supplierSkuCode && supplierSkuCodes.has(supplierSkuCode)) {
      errors.push("供应商 SKU 编码在文件内重复")
    }
    if (productKind && !PRODUCT_KIND_VALUES.includes(productKind as ProductKind)) {
      errors.push("商品类型填写有误（请填：实物/虚拟/服务/卡券）")
    }
    if (!isMoney(dropship)) errors.push("一件代发底价格式错误")
    if (!isMoney(bulk)) errors.push("集采底价格式错误")
    if (!isQuantity(moq)) errors.push("集采起订量格式错误")
    if (!isQuantity(available)) errors.push("可供数量格式错误")
    if (!isDate(validFrom) || !isDate(validTo)) errors.push("有效期必须为 YYYY-MM-DD")
    if (!["AVAILABLE", "UNAVAILABLE", "STOPPED", "STALE"].includes(availabilityStatus)) {
      errors.push("可供状态填写有误（请填：可供/不可供/已停供/信息待更新）")
    }

    const existing = products.get(supplierSpuCode)
    if (existing && (existing.name !== name || existing.sourceProductKind !== (productKind || undefined))) {
      errors.push("同一 SPU 的商品名称或商品类型不一致")
    }
    if (errors.length > 0) {
      rejectedRows.push({ rowNo, supplierSkuCode, errorText: errors.join("；") })
      return
    }

    supplierSkuCodes.add(supplierSkuCode)
    const sku: ParsedExcelSkuRow = {
      rowNo,
      supplierSkuCode,
      name,
      specification,
      sourceBaseUnit: value(row, "sourceBaseUnit") || undefined,
      barcode: value(row, "barcode") || undefined,
      attributes: parseAttributes(value(row, "attributes")),
      mainImageUrl: value(row, "mainImageUrl") || undefined,
      dropshipFloorPriceGross: dropship || undefined,
      bulkFloorPriceGross: bulk || undefined,
      bulkMinimumOrderQuantity: moq || undefined,
      availableQuantity: available || undefined,
      availabilityStatus: availabilityStatus as ParsedExcelSkuRow["availabilityStatus"],
    }
    if (existing) {
      products.set(supplierSpuCode, { ...existing, skus: [...existing.skus, sku] })
      return
    }
    products.set(supplierSpuCode, {
      supplierSpuCode,
      name,
      description: value(row, "description") || undefined,
      sourceProductKind: (productKind || undefined) as ProductKind | undefined,
      sourceCategory: value(row, "sourceCategory") || undefined,
      sourceBrand: value(row, "sourceBrand") || undefined,
      attributes: parseAttributes(value(row, "attributes")),
      media: mediaFromUrls(value(row, "carouselUrls"), value(row, "detailUrls")),
      sourceRevisionToken: value(row, "sourceRevisionToken") || undefined,
      validFrom: validFrom || undefined,
      validTo: validTo || undefined,
      skus: [sku],
    })
  })

  return {
    fileName: file.name,
    totalRows,
    products: Array.from(products.values()),
    rejectedRows,
  }
}
