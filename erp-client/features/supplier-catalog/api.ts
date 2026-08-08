/**
 * W21 · 供应商商品库 · 真实 HTTP 适配层。
 * 后端提供实体 CRUD（商品/SKU/映射/供给）；队列/工作项投影在 api.ts 内 thrift 组装。
 * 保持对外导出签名稳定。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
  CreateCompanyProductFromSupplierSkuInput,
  CreateSupplierCatalogItemInput,
  FormalActionResponse,
  LinkPromoteToCompanyPoolInput,
  PromoteSupplierProductInput,
  ReversePromoteToCompanyPoolInput,
  ReviseSupplierCatalogProductInput,
  ReviseSupplierOfferingInput,
  SessionCatalogDraft,
  SupplierCatalogCenterView,
  SupplierCatalogExceptionWorkItem,
  SupplierCatalogItemView,
  SupplierCatalogQueueQuery,
  SupplierCatalogQueueView,
  SupplierCatalogSkuView,
  SupplierCatalogWriteResult,
  SupplierCatalogWorkItemAction,
  SupplierCatalogDecision,
  SupplierOfferingRevisionView,
  SupplierProductMappingView,
  SupplierProductPoolMatchView,
  SupplierProductRevisionView,
  WorkItemLease,
} from "@/features/supplier-catalog/types"
import {
  REGISTRATION_BLOCKER_MESSAGE,
} from "@/features/supplier-catalog/types"
import type {
  SupplierCatalogExcelImportInput,
  SupplierCatalogExcelImportResult,
} from "@/features/supplier-catalog/excel-import"
import { uploadFileAssetImage } from "@/features/file-assets/api"

/** 供应商商品图片上传：复用共享文件资产上传（D05）。 */
export const uploadCatalogImage = uploadFileAssetImage
/** Excel 原始文件包含采购成本，按敏感附件登记。 */
export const uploadCatalogExcel = (file: File) =>
  uploadFileAssetImage(file, "attachment", "sensitive")
import {
  PRODUCT_KIND_VALUES,
  type ProductKind,
} from "@/features/master-data/types"

// ─── Backend wire types ───────────────────────────────────────────────────────

type BackendProduct = {
  id: string
  supplier_id: string
  source_type: "EXCEL" | "API" | "MANUAL" | string
  supplier_spu_code: string
  status: "ACTIVE" | "STOPPED" | "EXCEPTION" | string
  current_revision_id?: string | null
  current_revision_no?: number | null
  name?: string | null
  source_category?: string | null
  source_brand?: string | null
  source_updated_at?: number | null
  version: number
  created_at: number
}

type BackendSku = {
  id: string
  supplier_catalog_product_id: string
  supplier_sku_code: string
  status: string
  current_revision_id?: string | null
  current_revision_no?: number | null
  name?: string | null
  specification?: string | null
  source_base_unit?: string | null
  barcode?: string | null
  structured_attributes?: Array<{
    attribute_name: string
    attribute_value: string
  }>
  source_main_image_url?: string | null
  source_main_image_asset_id?: string | null
  dropship_floor_price_gross?: string | null
  bulk_floor_price_gross?: string | null
  bulk_minimum_order_quantity?: string | null
  available_quantity?: string | null
  availability_status?: string | null
  version: number
  created_at: number
}

type BackendSkuRevision = {
  id: string
  revision_no: number
  name: string
  specification: string
  source_base_unit?: string | null
  barcode?: string | null
  structured_attributes?: Array<{
    attribute_name: string
    attribute_value: string
  }>
  source_main_image_url?: string | null
  source_main_image_asset_id?: string | null
  dropship_floor_price_gross?: string | null
  bulk_floor_price_gross?: string | null
  bulk_minimum_order_quantity?: string | null
  available_quantity?: string | null
  availability_status: string
  source_updated_at: number
}

type BackendProductDetail = {
  product: BackendProduct
  revisions: Array<{
    id: string
    revision_no: number
    name: string
    description?: string | null
    source_product_kind?: string | null
    source_category?: string | null
    source_brand?: string | null
    structured_attributes: Array<{
      attribute_name: string
      attribute_value: string
    }>
    source_revision_token?: string | null
    source_updated_at: number
    valid_from?: string | null
    valid_to?: string | null
  }>
  media: Array<{
    id: string
    usage: string
    url?: string | null
    file_asset_id?: string | null
    archive_status: string
    sort_order: number
  }>
  skus: Array<{
    sku: BackendSku
    revisions: BackendSkuRevision[]
  }>
  mappings: BackendMapping[]
}

type BackendMapping = {
  id: string
  supplier_catalog_sku_id: string
  sku_id: string
  status: "PENDING" | "ACTIVE" | "CONFLICT" | "DISABLED" | string
  approved_by?: string | null
  approved_at?: number | null
  reason?: string | null
  version: number
  created_at: number
}

type BackendOffering = {
  id: string
  sku_id: string
  supplier_id: string
  supplier_catalog_sku_id: string
  status: "ACTIVE" | "PAUSED" | "STOPPED" | string
  current_revision_id?: string | null
  current_revision_no?: number | null
  dropship_supply_price_gross?: string | null
  dropship_supply_price_net?: string | null
  bulk_supply_price_gross?: string | null
  bulk_supply_price_net?: string | null
  input_tax_rate?: string | null
  bulk_minimum_order_quantity?: string | null
  supply_region: string[]
  availability_status?: string | null
  available_quantity?: string | null
  dropship_express?: string | null
  freight_amount?: string | null
  service_fee_amount?: string | null
  product_capabilities?: string[]
  valid_from?: string | null
  valid_to?: string | null
  version: number
  created_at: number
}

type BackendWorkItem = {
  id: string
  work_item_type: string
  business_object_type: string
  business_object_id: string
  subject_version?: string | null
  status: string
  owner_user_id?: string | null
  priority: string | number
  due_at?: number | null
  reason_code?: string | null
  impact_summary?: string | null
  completion_action: string
  version: number
  created_at: number
}

type BackendPublication = {
  id: string
  sku_id: string
  status: string
}

type BackendSkuListItem = {
  id: string
  sku_no: string
  product_id: string
  base_unit_id: string
  specification_signature: string
  status: string
  created_at: number
  version: number
}

type BackendCompanyProduct = {
  id: string
  product_kind: ProductKind
}

type BackendUnitOfMeasure = {
  id: string
  unit_code: string
  name: string
  symbol: string
  quantity_scale: number
  status: string
  created_at: number
  version: number
}

/** 公司 SKU 最新修订（销售可见价/名称/条码所在）。 */
type CompanySkuRevisionDto = {
  id: string
  sku_id: string
  revision_no: number
  name: string
  barcode: string | null
  status: string
  sales_visible_price_gross: string | null
  effective_from: string
  created_at: number
  version: number
}

/** 公司 SKU 富化信息：编码/名称/规格/单位/商品池价。 */
type CompanySkuEnrichment = {
  skuId: string
  productId: string
  productKind: ProductKind
  skuCode: string
  skuName: string
  specification: string
  baseUnit: string
  barcode?: string
  salesVisiblePriceGross?: string
  poolEntryId?: string
  poolEntryRevisionId?: string
}

/** 分页拉全后端列表（与 master-data 同款辅助，避免 100 条截断）。 */
async function fetchAllPages<T>(
  path: string,
  query: Record<string, unknown> = {}
): Promise<T[]> {
  const items: T[] = []
  let page = 1
  let total = Number.POSITIVE_INFINITY
  while (items.length < total) {
    const result = await apiGet<Page<T>>(path, {
      ...query,
      page,
      page_size: 100,
    })
    items.push(...result.items)
    total = result.total
    if (result.items.length === 0) break
    page += 1
  }
  return items
}

/**
 * 加载公司 SKU 富化信息（编码/名称/规格/单位/商品池价）。
 * 只对关心的公司 SKU 逐个取最新修订（page_size 1 + revision_no 倒序），
 * 避免全量 sku-revisions 翻页截断；调用方传空数组时拉全部。
 */
async function loadCompanySkuEnrichment(
  skuIds: string[]
): Promise<Map<string, CompanySkuEnrichment>> {
  const [skus, units, products] = await Promise.all([
    fetchAllPages<BackendSkuListItem>("/admin/skus", {}).catch(() => []),
    fetchAllPages<BackendUnitOfMeasure>("/admin/unit-of-measures", {}).catch(
      () => []
    ),
    fetchAllPages<BackendCompanyProduct>("/admin/products", {}).catch(() => []),
  ])
  const unitById = new Map(units.map((unit) => [unit.id, unit]))
  const productById = new Map(products.map((product) => [product.id, product]))
  const ids = skuIds.length > 0 ? skuIds : skus.map((sku) => sku.id)
  const latestBySku = new Map<string, CompanySkuRevisionDto>()
  await Promise.all(
    ids.map(async (skuId) => {
      try {
        const page = await apiGet<Page<CompanySkuRevisionDto>>(
          "/admin/sku-revisions",
          {
            sku_id: skuId,
            page: 1,
            page_size: 1,
            sort_by: "revision_no",
            sort_dir: "desc",
          }
        )
        const latest = page.items[0]
        if (latest) latestBySku.set(skuId, latest)
      } catch {
        // 无权限或不存在时省略
      }
    })
  )
  const byId = new Map<string, CompanySkuEnrichment>()
  for (const sku of skus) {
    const latest = latestBySku.get(sku.id)
    const unit = unitById.get(sku.base_unit_id)
    const product = productById.get(sku.product_id)
    if (!product) continue
    byId.set(sku.id, {
      skuId: sku.id,
      productId: sku.product_id,
      productKind: product.product_kind,
      skuCode: sku.sku_no,
      skuName: latest?.name ?? sku.sku_no,
      specification: sku.specification_signature,
      baseUnit:
        unit?.name ?? unit?.symbol ?? unit?.unit_code ?? sku.base_unit_id,
      barcode: latest?.barcode ?? undefined,
      salesVisiblePriceGross: latest?.sales_visible_price_gross ?? undefined,
      poolEntryId:
        latest?.sales_visible_price_gross != null ? sku.id : undefined,
      poolEntryRevisionId:
        latest?.sales_visible_price_gross != null ? latest.id : undefined,
    })
  }
  return byId
}

// ─── Session draft (client-only, not mock seed) ───────────────────────────────

const drafts = new Map<string, SessionCatalogDraft>()

// ─── Helpers ──────────────────────────────────────────────────────────────────

function secsToIso(secs?: number | null): string {
  if (secs == null || secs <= 0) return new Date(0).toISOString()
  return new Date(secs * 1000).toISOString()
}

function todayIso(): string {
  return new Date().toISOString().slice(0, 10)
}

function sourceTypeLabel(t: string): string {
  if (t === "EXCEL") return "Excel"
  if (t === "API") return "API"
  if (t === "MANUAL") return "手工"
  return t
}

function changeTypeFromState(
  status: string,
  mapping?: BackendMapping,
  offering?: BackendOffering
): "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "UNCHANGED" {
  if (status === "STOPPED") return "STOPPED"
  if (status === "EXCEPTION") return "ERROR"
  if (!mapping || mapping.status !== "ACTIVE") return "NEW"
  if (!offering || offering.status !== "ACTIVE") return "CHANGED"
  return "UNCHANGED"
}

function workItemPriority(value: string | number): number {
  if (typeof value === "number") return value
  if (value === "URGENT") return 100
  if (value === "HIGH") return 75
  if (value === "LOW") return 25
  return 50
}

function mapCatalogWorkItem(
  row: BackendWorkItem,
  product: BackendProduct,
  offering?: BackendOffering
): SupplierCatalogExceptionWorkItem {
  const held = row.status === "DEFERRED"
  return {
    workItemId: row.id,
    workItemType: "BUSINESS_EXCEPTION",
    businessObjectType:
      offering && row.business_object_id === offering.id
        ? "SUPPLIER_OFFERING"
        : "SUPPLIER_CATALOG_SKU",
    subjectVersion: row.subject_version ?? String(product.version),
    subjectHash: row.business_object_id,
    workItemStatus:
      row.status === "COMPLETED" || row.status === "CLOSED"
        ? "COMPLETED"
        : row.status === "PENDING"
          ? "PENDING"
          : "IN_PROGRESS",
    dueAt: row.due_at ? secsToIso(row.due_at) : undefined,
    claimedBy: row.owner_user_id
      ? { userId: row.owner_user_id, displayName: row.owner_user_id }
      : undefined,
    allowedActions: [
      "CLAIM",
      "HOLD",
      "RETURN_FOR_DATA_FIX",
      "QUERY_ORIGINAL_RESULT",
      "SAVE_EVIDENCE",
      "CONFIRM_ERROR_RESOLVED",
      "CONFIRM_STOP_SUPPLY",
    ],
    actionBlockers: [],
    held,
    reason: row.reason_code ?? "供应商商品业务异常",
    impact: row.impact_summary ?? "需采购复核",
    priority: workItemPriority(row.priority),
    handlerKey: "supplier_catalog",
  }
}

function mapAvailability(
  raw?: string | null
): "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE" {
  const u = (raw ?? "AVAILABLE").toUpperCase()
  if (u === "UNAVAILABLE" || u === "STOPPED" || u === "STALE") return u
  return "AVAILABLE"
}

function mapProductKind(
  value: string | null | undefined,
): ProductKind | undefined {
  return PRODUCT_KIND_VALUES.includes(value as ProductKind)
    ? (value as ProductKind)
    : undefined
}

function publicationImpact(
  companySkuId?: string,
  publications?: BackendPublication[] | null
) {
  const related = companySkuId
    ? publications?.filter((publication) => publication.sku_id === companySkuId)
    : undefined
  const activePublicationCount = related
    ? related.filter((publication) =>
        ["mall_effective", "MALL_EFFECTIVE"].includes(publication.status)
      ).length
    : null
  const pausedPublicationCount = related
    ? related.filter((publication) =>
        ["paused", "PAUSED"].includes(publication.status)
      ).length
    : null
  return {
    activePublicationCount,
    pausedPublicationCount,
    historicalPaidOrderCount: null,
    safetyPauseTriggered: false,
    safetyPauseReasons: [] as string[],
    pauseSubResults: [] as Array<{
      id: string
      publicationId: string
      reason: string
      outboxId: string
      status: string
    }>,
    mallSalePriceAutoUpdate: false as const,
    moqCopiedToMallMinPurchase: false as const,
    note: companySkuId
      ? publications
        ? "发布状态按当前公司 SKU 实时聚合；历史已支付订单由订单域保留，不在本读取中推断。"
        : "当前账号无法读取商品发布聚合。"
      : "供应商 SKU 尚未关联公司 SKU，不产生商城发布影响。",
  }
}

function fileNameFromMediaUrl(url: string | null | undefined): string {
  const value = (url ?? "").trim()
  if (!value) return ""
  try {
    const parsed = new URL(value)
    const last = parsed.pathname.split("/").filter(Boolean).pop()
    if (last) return decodeURIComponent(last)
  } catch {
    // not a full URL — fall through
  }
  const slash = value.split(/[\\/]/).filter(Boolean).pop()
  return slash || value
}

function mapMediaList(
  media: BackendProductDetail["media"] | undefined,
): NonNullable<SupplierProductRevisionView["media"]> {
  return (media ?? []).map((entry, index) => {
    const url = entry.url?.trim() || undefined
    const fileName = fileNameFromMediaUrl(url) || `media-${index + 1}`
    const usageRaw = (entry.usage ?? "").toUpperCase()
    const usage =
      usageRaw === "SPU_DETAIL"
        ? ("SPU_DETAIL" as const)
        : usageRaw === "SKU_MAIN"
          ? ("SKU_MAIN" as const)
          : ("SPU_CAROUSEL" as const)
    const archive =
      (entry.archive_status ?? "").toUpperCase() === "ARCHIVED"
        ? ("ARCHIVED" as const)
        : (entry.archive_status ?? "").toUpperCase() === "FAILED"
          ? ("FAILED" as const)
          : ("PENDING_IMPORT" as const)
    return {
      id: entry.id,
      usage,
      fileName,
      sortOrder: entry.sort_order ?? index,
      sourceUrl: url,
      fileAssetId: entry.file_asset_id?.trim() || undefined,
      archiveStatus: archive,
    }
  })
}

function mapStructuredAttributes(
  attrs:
    | Array<{ attribute_name: string; attribute_value: string }>
    | null
    | undefined,
): SupplierProductRevisionView["attributes"] {
  return (attrs ?? [])
    .map((attr) => ({
      name: attr.attribute_name?.trim() ?? "",
      value: attr.attribute_value?.trim() ?? "",
    }))
    .filter((attr) => attr.name && attr.value)
}

function mapSkuMainMedia(
  url: string | null | undefined,
  fileAssetId?: string | null,
): NonNullable<SupplierProductRevisionView["media"]> {
  const sourceUrl = url?.trim()
  if (!sourceUrl) return []
  return [
    {
      id: `sku-main:${sourceUrl}`,
      usage: "SKU_MAIN" as const,
      fileName: fileNameFromMediaUrl(sourceUrl) || "main",
      sortOrder: 0,
      sourceUrl,
      fileAssetId: fileAssetId?.trim() || undefined,
      archiveStatus: "PENDING_IMPORT" as const,
    },
  ]
}

function mapSkuRevision(
  rev: BackendSkuRevision | undefined,
  productName: string,
  category: string,
  brand: string | undefined,
  extras?: {
    description?: string | null
    sourceProductKind?: string | null
    media?: NonNullable<SupplierProductRevisionView["media"]>
    attributes?: NonNullable<SupplierProductRevisionView["attributes"]>
  },
): SupplierProductRevisionView {
  const r = rev
  const skuMedia = mapSkuMainMedia(
    r?.source_main_image_url,
    r?.source_main_image_asset_id,
  )
  const media = [...(extras?.media ?? []), ...skuMedia]
  return {
    revisionNo: r?.revision_no ?? 1,
    sourceUpdatedAt: secsToIso(r?.source_updated_at),
    syncedAt: secsToIso(r?.source_updated_at),
    name: r?.name ?? productName,
    description: extras?.description ?? undefined,
    sourceProductKind: mapProductKind(extras?.sourceProductKind),
    specification: r?.specification ?? "",
    category,
    brand,
    baseUnit: r?.source_base_unit ?? undefined,
    barcode: r?.barcode ?? undefined,
    attributes: (() => {
      const fromSku = mapStructuredAttributes(r?.structured_attributes)
      return fromSku && fromSku.length > 0 ? fromSku : extras?.attributes
    })(),
    media: media.length > 0 ? media : extras?.media,
    dropshipFloorPriceGross: r?.dropship_floor_price_gross ?? null,
    bulkFloorPriceGross: r?.bulk_floor_price_gross ?? null,
    bulkMinimumOrderQuantity: r?.bulk_minimum_order_quantity ?? null,
    availableQuantity: r?.available_quantity ?? "0",
    availabilityStatus: mapAvailability(r?.availability_status),
  }
}

/** 写入媒体：后端只接收 usage + 非空 url；文件资产 id 可选。 */
function mediaToWritePayload(
  media: readonly {
    usage: string
    sourceUrl?: string
    fileName?: string
    fileAssetId?: string
  }[],
): Array<{ usage: string; url: string; file_asset_id?: string }> {
  return media
    .map((entry) => {
      const usage = entry.usage?.toUpperCase()
      if (usage !== "SPU_CAROUSEL" && usage !== "SPU_DETAIL") return null
      const url = (entry.sourceUrl ?? entry.fileName ?? "").trim()
      if (!url) return null
      return {
        usage,
        url,
        ...(entry.fileAssetId?.trim()
          ? { file_asset_id: entry.fileAssetId.trim() }
          : {}),
      }
    })
    .filter(
      (entry): entry is { usage: string; url: string; file_asset_id?: string } =>
        entry != null,
    )
}

/** SKU 主图写入信息：来源地址 + 已登记文件资产。 */
function skuMainImageFromWrite(input: {
  media?: readonly { usage: string; sourceUrl?: string; fileName?: string; fileAssetId?: string }[]
  mainImage?: string
}): { url: string | null; assetId?: string } {
  const fromMedia = (input.media ?? []).find(
    (entry) => entry.usage?.toUpperCase() === "SKU_MAIN",
  )
  const url =
    fromMedia?.sourceUrl?.trim() ||
    fromMedia?.fileName?.trim() ||
    input.mainImage?.trim() ||
    ""
  const assetId = fromMedia?.fileAssetId?.trim()
  return {
    url: url || null,
    ...(assetId ? { assetId } : {}),
  }
}

function mapOffering(o: BackendOffering): SupplierOfferingRevisionView {
  return {
    offeringId: o.id,
    offeringRevisionId: o.current_revision_id ?? o.id,
    revisionNo: o.current_revision_no ?? 1,
    status:
      o.status === "PAUSED"
        ? "PAUSED"
        : o.status === "STOPPED"
          ? "STOPPED"
          : "ACTIVE",
    supplyPriceGross:
      o.dropship_supply_price_gross ?? o.bulk_supply_price_gross ?? null,
    supplyPriceNet:
      o.dropship_supply_price_net ?? o.bulk_supply_price_net ?? null,
    floorPriceGross: null,
    dropshipSupplyPriceGross: o.dropship_supply_price_gross ?? null,
    bulkSupplyPriceGross: o.bulk_supply_price_gross ?? null,
    inputTaxRate: o.input_tax_rate ?? null,
    freightAmount: o.freight_amount ?? null,
    serviceFeeAmount: o.service_fee_amount ?? null,
    minimumOrderQuantity: o.bulk_minimum_order_quantity ?? "1",
    supplyRegion: o.supply_region ?? [],
    availabilityStatus: o.availability_status ?? "AVAILABLE",
    availableQuantity: o.available_quantity ?? "0",
    productCapabilities: o.product_capabilities ?? [],
    dropshipExpress: o.dropship_express ?? undefined,
    validFrom: o.valid_from ?? secsToIso(o.created_at).slice(0, 10),
    validTo: o.valid_to ?? undefined,
    createdAt: secsToIso(o.created_at),
    immutable: true,
  }
}

function mapMapping(
  m: BackendMapping,
  enrichment?: CompanySkuEnrichment
): SupplierProductMappingView {
  return {
    mappingStatus:
      m.status === "ACTIVE"
        ? "ACTIVE"
        : m.status === "CONFLICT"
          ? "CONFLICT"
          : m.status === "DISABLED"
            ? "DISABLED"
            : "PENDING",
    skuId: m.sku_id,
    skuCode: enrichment?.skuCode,
    skuName: enrichment?.skuName,
    skuRevisionId: enrichment?.poolEntryRevisionId,
    specification: enrichment?.specification,
    baseUnit: enrichment?.baseUnit,
    approvedBy: m.approved_by ?? undefined,
    approvedAt: m.approved_at ? secsToIso(m.approved_at) : undefined,
    reason: m.reason ?? undefined,
    mappingVersion: String(m.version),
    history: [],
  }
}

async function resolveSupplierName(supplierId: string): Promise<string> {
  try {
    const row = await apiGet<{ id: string; name?: string; supplier_name?: string }>(
      `/admin/suppliers/${encodeURIComponent(supplierId)}`
    )
    return row.name ?? row.supplier_name ?? supplierId
  } catch {
    return supplierId
  }
}

function projectProductToItem(
  product: BackendProduct,
  skus: BackendSku[],
  skuRevisions: Map<string, BackendSkuRevision>,
  mappings: BackendMapping[],
  offerings: BackendOffering[],
  workItems: BackendWorkItem[],
  publications: BackendPublication[] | null,
  supplierName: string,
  companySkuById?: Map<string, CompanySkuEnrichment>,
): SupplierCatalogItemView {
  const primarySku = skus[0]
  const primaryRev = primarySku
    ? skuRevisions.get(primarySku.id)
    : undefined
  const category = product.source_category ?? ""
  const brand = product.source_brand ?? undefined
  // 列表投影的 content 多挂在主 SKU 修订上，但「来源版本号」必须以 SPU 当前修订为准：
  // 反向入池 expected_source_revision_no 校验的是 supplier_catalog_product_revision，
  // 不可误传 SKU 修订号（常见为 1）。
  const productSourceRevisionNo = product.current_revision_no ?? 1
  const currentRevision = {
    ...mapSkuRevision(
      primaryRev,
      product.name ?? product.supplier_spu_code,
      category,
      brand
    ),
    revisionNo: productSourceRevisionNo,
  }

  const catalogSkus: SupplierCatalogSkuView[] = skus.map((s) => ({
    id: s.id,
    supplierSkuCode: s.supplier_sku_code,
    currentRevision: mapSkuRevision(
      skuRevisions.get(s.id) ??
        ({
          id: s.id,
          revision_no: s.current_revision_no ?? 1,
          name: s.name ?? product.name ?? s.supplier_sku_code,
          specification: s.specification ?? "",
          barcode: s.barcode,
          source_base_unit: s.source_base_unit ?? null,
          structured_attributes: s.structured_attributes ?? [],
          source_main_image_url: s.source_main_image_url,
          source_main_image_asset_id: s.source_main_image_asset_id,
          dropship_floor_price_gross: s.dropship_floor_price_gross,
          bulk_floor_price_gross: s.bulk_floor_price_gross,
          bulk_minimum_order_quantity: s.bulk_minimum_order_quantity,
          available_quantity: s.available_quantity ?? null,
          availability_status: s.availability_status ?? "AVAILABLE",
          source_updated_at: product.source_updated_at ?? product.created_at,
        } as BackendSkuRevision),
      product.name ?? s.supplier_sku_code,
      category,
      brand
    ),
  }))

  const skuIds = new Set(skus.map((s) => s.id))
  const mapping =
    mappings.find(
      (candidate) =>
        candidate.status === "ACTIVE" &&
        skuIds.has(candidate.supplier_catalog_sku_id)
    ) ?? mappings.find((candidate) => skuIds.has(candidate.supplier_catalog_sku_id))
  const offering =
    offerings.find(
      (candidate) =>
        candidate.status === "ACTIVE" &&
        skuIds.has(candidate.supplier_catalog_sku_id)
    ) ?? offerings.find((candidate) => skuIds.has(candidate.supplier_catalog_sku_id))
  const changeType = changeTypeFromState(product.status, mapping, offering)

  const mappingEnrichment = mapping?.sku_id
    ? companySkuById?.get(mapping.sku_id)
    : undefined
  const mappedCandidate = mapping?.sku_id && mappingEnrichment
    ? {
        productId: mappingEnrichment.productId,
        skuId: mappingEnrichment.skuId,
        skuCode: mappingEnrichment.skuCode,
        skuName: mappingEnrichment.skuName,
        specification: mappingEnrichment.specification,
        baseUnit: mappingEnrichment.baseUnit,
        barcode: mappingEnrichment.barcode,
        revisionNo: 1,
        similarityLabel: "已关联公司 SKU",
        poolEntry: mappingEnrichment.poolEntryId
          ? {
              poolEntryId: mappingEnrichment.poolEntryId,
              poolEntryRevisionId: mappingEnrichment.poolEntryRevisionId ?? "",
              status: "ACTIVE" as const,
              salesVisiblePriceGross:
                mappingEnrichment.salesVisiblePriceGross ?? "",
              validFrom: todayIso(),
            }
          : undefined,
      }
    : undefined

  const base = {
    supplierProduct: {
      id: product.id,
      supplier: { id: product.supplier_id, name: supplierName },
      source: {
        type: product.source_type as "EXCEL" | "API" | "MANUAL",
        label: sourceTypeLabel(product.source_type),
      },
      supplierSpuCode: product.supplier_spu_code,
      supplierSkuCode:
        primarySku?.supplier_sku_code ?? product.supplier_spu_code,
      status: product.status,
      currentRevision,
      catalogSkus,
    },
    mapping: mapping ? mapMapping(mapping, mappingEnrichment) : undefined,
    skuCandidates: mappedCandidate ? [mappedCandidate] : [],
    poolEntry: mappedCandidate?.poolEntry,
    offering: offering
      ? {
          stableId: offering.id,
          currentRevision: mapOffering(offering),
          revisionHistory: [mapOffering(offering)],
          proposedDefaults: {
            dropshipSupplyPriceGross:
              offering.dropship_supply_price_gross ?? "",
            bulkSupplyPriceGross: offering.bulk_supply_price_gross ?? "",
            dropshipExpress: offering.dropship_express ?? undefined,
            inputTaxRate: offering.input_tax_rate ?? "",
            freightAmount: offering.freight_amount ?? "0.00",
            serviceFeeAmount: offering.service_fee_amount ?? "0.00",
            minimumOrderQuantity:
              offering.bulk_minimum_order_quantity ?? "1",
            supplyRegion: offering.supply_region ?? [],
            productCapabilities: offering.product_capabilities ?? [],
            validFrom:
              offering.valid_from ?? secsToIso(offering.created_at).slice(0, 10),
            validTo: offering.valid_to ?? undefined,
            sessionDraftOnly: true as const,
          },
        }
      : undefined,
    publicationImpact: publicationImpact(mapping?.sku_id, publications),
    sourceContext: {
      intakeId: product.id,
      sourceReference: product.supplier_spu_code,
      receivedAt: secsToIso(product.created_at),
    },
    sourceDiff: [] as const,
    allowedActions: [
      "OPEN_CENTER",
      "CREATE_MAPPING",
      "PROMOTE_TO_POOL",
      "REVISE_PRODUCT",
    ],
    actionBlockers: [] as Array<{
      action: string
      code: string
      message: string
      destinationWorkspaceId?: string
    }>,
  }

  if (changeType === "ERROR" || changeType === "STOPPED") {
    const workItem = workItems.find(
      (candidate) =>
        skuIds.has(candidate.business_object_id) ||
        candidate.business_object_id === offering?.id
    )
    if (!workItem) {
      return {
        ...base,
        changeType,
        registrationBlocker: {
          code: "WORK_ITEM_TYPE_UNREGISTERED",
          message:
            "异常或停供记录尚未派发正式待办，领取、暂挂和完成动作不可用。",
          businessProcess: "OFFERING_REVIEW",
        },
      }
    }
    return {
      ...base,
      changeType,
      workItem: mapCatalogWorkItem(workItem, product, offering),
    }
  }

  if (changeType === "NEW" || changeType === "CHANGED") {
    return {
      ...base,
      changeType,
      registrationBlocker: {
        code: "WORK_ITEM_TYPE_UNREGISTERED",
        message: REGISTRATION_BLOCKER_MESSAGE,
        businessProcess:
          mapping?.status === "ACTIVE" ? "OFFERING_REVIEW" : "MAPPING",
      },
    }
  }

  return {
    ...base,
    changeType: "UNCHANGED",
  }
}

async function loadQueueItems(
  query: SupplierCatalogQueueQuery
): Promise<SupplierCatalogItemView[]> {
  const productPages = await fetchAllPages<BackendProduct>(
    "/admin/supplier-catalog/products",
    {
      q: query.q?.trim() || undefined,
      supplier_id: query.supplierId || undefined,
      source_type:
        query.sourceType && query.sourceType !== "all"
          ? query.sourceType
          : undefined,
    }
  )

  const items: SupplierCatalogItemView[] = []

  // 目录、映射、供给与待办都完整翻页，队列总数和当前项不能由第一页猜测。
  const [allSkus, allMappings, allOfferings, allWorkItems, allPublications] =
    await Promise.all([
    fetchAllPages<BackendSku>("/admin/supplier-catalog/skus").catch(
      () => [] as BackendSku[]
    ),
    fetchAllPages<BackendMapping>("/admin/supplier-catalog/mappings").catch(
      () => [] as BackendMapping[]
    ),
    fetchAllPages<BackendOffering>("/admin/supplier-catalog/offerings").catch(
      () => [] as BackendOffering[]
    ),
    fetchAllPages<BackendWorkItem>("/admin/work-items", {
      work_item_type: "BUSINESS_EXCEPTION",
    }).catch(() => [] as BackendWorkItem[]),
    fetchAllPages<BackendPublication>("/admin/product-publications").catch(
      () => null
    ),
  ])
  const skusByProduct = new Map<string, BackendSku[]>()
  for (const sku of allSkus) {
    const group = skusByProduct.get(sku.supplier_catalog_product_id) ?? []
    group.push(sku)
    skusByProduct.set(sku.supplier_catalog_product_id, group)
  }
  const mappedSkuIds = new Set<string>()
  for (const mapping of allMappings) {
    if (mapping.sku_id) mappedSkuIds.add(mapping.sku_id)
  }
  const companySkuById = await loadCompanySkuEnrichment(
    Array.from(mappedSkuIds)
  )
  const supplierNames = new Map<string, string>()
  await Promise.all(
    Array.from(new Set(productPages.map((product) => product.supplier_id))).map(
      async (supplierId) => {
        supplierNames.set(supplierId, await resolveSupplierName(supplierId))
      }
    )
  )

  for (const product of productPages) {
    const skuPage = skusByProduct.get(product.id) ?? []
    const skuIds = new Set(skuPage.map((sku) => sku.id))
    const mappings = allMappings.filter((mapping) =>
      skuIds.has(mapping.supplier_catalog_sku_id)
    )
    const offerings = allOfferings.filter((offering) =>
      skuIds.has(offering.supplier_catalog_sku_id)
    )

    const skuRevisions = new Map<string, BackendSkuRevision>()
    for (const s of skuPage) {
      skuRevisions.set(s.id, {
        id: s.current_revision_id ?? s.id,
        revision_no: s.current_revision_no ?? 1,
        name: s.name ?? product.name ?? s.supplier_sku_code,
        specification: s.specification ?? "",
        source_base_unit: s.source_base_unit ?? null,
        barcode: s.barcode,
        structured_attributes: s.structured_attributes ?? [],
        source_main_image_url: s.source_main_image_url,
        source_main_image_asset_id: s.source_main_image_asset_id,
        dropship_floor_price_gross: s.dropship_floor_price_gross,
        bulk_floor_price_gross: s.bulk_floor_price_gross,
        bulk_minimum_order_quantity: s.bulk_minimum_order_quantity,
        available_quantity: s.available_quantity ?? null,
        availability_status: s.availability_status ?? "AVAILABLE",
        source_updated_at: product.source_updated_at ?? product.created_at,
      })
    }

    const item = projectProductToItem(
      product,
      skuPage,
      skuRevisions,
      mappings,
      offerings,
      allWorkItems,
      allPublications,
      supplierNames.get(product.supplier_id) ?? product.supplier_id,
      companySkuById
    )
    items.push(item)
  }

  let filtered = items

  if (!query.changeType || query.changeType === "actionable") {
    filtered = filtered.filter((i) => i.changeType !== "UNCHANGED")
  } else if (query.changeType !== "all") {
    filtered = filtered.filter((i) => i.changeType === query.changeType)
  }

  if (query.skuId) {
    filtered = filtered.filter(
      (i) =>
        i.mapping?.skuId === query.skuId ||
        i.skuCandidates.some((c) => c.skuId === query.skuId)
    )
  }

  if (query.mappingStatus && query.mappingStatus !== "all") {
    filtered = filtered.filter(
      (item) =>
        (item.mapping?.mappingStatus ?? "UNMAPPED") === query.mappingStatus
    )
  }

  if (query.offeringStatus && query.offeringStatus !== "all") {
    filtered = filtered.filter(
      (item) =>
        (item.offering?.currentRevision?.status ?? "MISSING") ===
        query.offeringStatus
    )
  }

  if (query.q?.trim()) {
    const q = query.q.trim().toUpperCase()
    filtered = filtered.filter((i) => {
      const ep = i.supplierProduct
      return (
        ep.supplierSpuCode?.toUpperCase().includes(q) ||
        ep.supplierSkuCode.toUpperCase().includes(q) ||
        ep.currentRevision.name.toUpperCase().includes(q) ||
        i.mapping?.skuCode?.toUpperCase().includes(q) ||
        ep.supplier.name.includes(query.q!.trim())
      )
    })
  }

  return filtered
}

function hasCatalogWorkItem(
  item: SupplierCatalogItemView
): item is SupplierCatalogItemView & {
  workItem: SupplierCatalogExceptionWorkItem
} {
  return "workItem" in item && item.workItem !== undefined
}

// ─── Public API ───────────────────────────────────────────────────────────────

export async function fetchSupplierCatalogQueue(
  query: SupplierCatalogQueueQuery
): Promise<SupplierCatalogQueueView> {
  const items = await loadQueueItems(query)

  const queueContextId =
    query.queueContextId ??
    `queue:W21:${query.changeType ?? "actionable"}:${query.skuId ?? "all"}`

  let position = 0
  let current = items[0]

  if (query.currentWorkItemId) {
    const idx = items.findIndex(
      (i) =>
        hasCatalogWorkItem(i) &&
        i.workItem.workItemId === query.currentWorkItemId
    )
    if (idx >= 0) {
      position = idx
      current = items[idx]
    }
  } else if (query.currentSupplierProductId) {
    const idx = items.findIndex(
      (i) => i.supplierProduct.id === query.currentSupplierProductId
    )
    if (idx >= 0) {
      position = idx
      current = items[idx]
    }
  }

  const emptyReason =
    items.length === 0
      ? query.q || query.skuId || query.changeType
        ? "FILTER_NO_RESULT"
        : "NO_TASKS"
      : undefined

  const currentWorkItemId =
    current && hasCatalogWorkItem(current)
      ? current.workItem.workItemId
      : undefined

  // skuContext：按稳定 skuId 精确加载，不能用第一页第一条猜测当前 SKU。
  let skuContext: SupplierCatalogQueueView["skuContext"]
  if (query.skuId) {
    try {
      const enrichment = await loadCompanySkuEnrichment([query.skuId])
      const hit = enrichment.get(query.skuId)
      if (hit) {
        skuContext = {
          productId: hit.productId,
          productName: hit.skuName,
          productKind: hit.productKind,
          skuId: hit.skuId,
          skuCode: hit.skuCode,
          specification: hit.specification,
          baseUnit: hit.baseUnit,
          barcode: hit.barcode,
          poolEntry: hit.poolEntryId && hit.poolEntryRevisionId && hit.salesVisiblePriceGross
            ? {
                poolEntryId: hit.poolEntryId,
                poolEntryRevisionId: hit.poolEntryRevisionId,
                status: "ACTIVE",
                salesVisiblePriceGross: hit.salesVisiblePriceGross,
              }
            : undefined,
        }
      }
    } catch {
      // 无权限或不存在时省略
    }
  }

  return {
    preferences: { autoNextDefault: true },
    skuContext,
    context: {
      queueContextId,
      position: items.length === 0 ? 0 : position + 1,
      total: items.length,
      currentSupplierProductId: current?.supplierProduct.id,
      currentWorkItemId,
      previousSupplierProductId: items[position - 1]?.supplierProduct.id,
      nextSupplierProductId: items[position + 1]?.supplierProduct.id,
      filterSummary: [
        query.sourceType && query.sourceType !== "all"
          ? sourceTypeLabel(query.sourceType)
          : null,
        query.changeType && query.changeType !== "all" && query.changeType !== "actionable"
          ? query.changeType
          : null,
        query.q?.trim() ? `搜索「${query.q.trim()}」` : null,
        `${items.length} 条`,
      ]
        .filter(Boolean)
        .join(" · "),
      queueContextUpdatedAt: new Date(0).toISOString(),
    },
    items,
    current,
    emptyReason,
  }
}

export async function fetchCompanySkuOptions() {
  const enrichment = await loadCompanySkuEnrichment([])

  return Array.from(enrichment.values()).map((sku) => ({
    productId: sku.productId,
    skuId: sku.skuId,
    skuCode: sku.skuCode,
    skuName: sku.skuName,
    specification: sku.specification,
    baseUnit: sku.baseUnit,
    barcode: sku.barcode,
    brand: undefined as string | undefined,
    category: undefined as string | undefined,
    revisionNo: 1,
    similarityLabel: "公司商品候选",
    activeSupplierCount: 0,
    poolEntry: sku.poolEntryId
      ? ({
          poolEntryId: sku.poolEntryId,
          poolEntryRevisionId: sku.poolEntryRevisionId ?? "",
          status: "ACTIVE" as const,
          salesVisiblePriceGross: sku.salesVisiblePriceGross ?? "",
        })
      : undefined,
  }))
}

export async function fetchSupplierCatalogCenter(input: {
  supplierProductId: string
  section?: string
}): Promise<SupplierCatalogCenterView | null> {

  let detail: BackendProductDetail
  try {
    detail = await apiGet<BackendProductDetail>(
      `/admin/supplier-catalog/products/${encodeURIComponent(input.supplierProductId)}`
    )
  } catch (err) {
    const e = err as { kind?: string; status?: number }
    if (e?.kind === "Http" && e.status === 404) return null
    throw err
  }

  const product = detail.product
  const skus = detail.skus.map((s) => s.sku)
  const skuRevisions = new Map<string, BackendSkuRevision>()
  for (const entry of detail.skus) {
    const latest = entry.revisions[0]
    if (latest) skuRevisions.set(entry.sku.id, latest)
  }

  const [offerings, workItems, publications, supplierName, companySkuById] =
    await Promise.all([
    fetchAllPages<BackendOffering>("/admin/supplier-catalog/offerings", {
      supplier_id: product.supplier_id,
    }).catch(() => [] as BackendOffering[]),
    fetchAllPages<BackendWorkItem>("/admin/work-items", {
      work_item_type: "BUSINESS_EXCEPTION",
    }).catch(() => [] as BackendWorkItem[]),
    fetchAllPages<BackendPublication>("/admin/product-publications").catch(
      () => null
    ),
    resolveSupplierName(product.supplier_id),
    loadCompanySkuEnrichment(
      detail.mappings
        .map((mapping) => mapping.sku_id)
        .filter((id): id is string => Boolean(id))
    ),
  ])

  const projectedItem = projectProductToItem(
    product,
    skus,
    skuRevisions,
    detail.mappings,
    offerings,
    workItems,
    publications,
    supplierName,
    companySkuById
  )

  const productRevision =
    detail.revisions.find(
      (revision) => revision.revision_no === product.current_revision_no,
    ) ?? detail.revisions[0]
  const productMedia = mapMediaList(detail.media)
  const productAttributes = mapStructuredAttributes(
    productRevision?.structured_attributes,
  )
  const productCategory =
    productRevision?.source_category ?? product.source_category ?? ""
  const productBrand =
    productRevision?.source_brand ?? product.source_brand ?? undefined
  const productName =
    productRevision?.name ?? product.name ?? product.supplier_spu_code

  const skuDetailById = new Map(
    detail.skus.map((entry) => [entry.sku.id, entry] as const),
  )
  const catalogSkus = (projectedItem.supplierProduct.catalogSkus ?? []).map(
    (skuView) => {
      const skuEntry = skuDetailById.get(skuView.id)
      const latestRev = skuEntry?.revisions?.[0]
      return {
        ...skuView,
        currentRevision: mapSkuRevision(
          latestRev,
          productName,
          productCategory,
          productBrand,
          {
            description: productRevision?.description,
            sourceProductKind: productRevision?.source_product_kind,
            // SPU 图文挂在产品修订；SKU 主图由 mapSkuRevision 从 latestRev 补齐
            media: productMedia,
            attributes: productAttributes,
          },
        ),
      }
    },
  )

  const primarySkuRev = detail.skus[0]?.revisions?.[0]
  // 详情页 content 可从主 SKU 补齐，但并发校验 revisionNo 必须是 SPU 来源修订号。
  const productSourceRevisionNo =
    productRevision?.revision_no ??
    product.current_revision_no ??
    projectedItem.supplierProduct.currentRevision.revisionNo
  const currentRevision = {
    ...mapSkuRevision(
      primarySkuRev,
      productName,
      productCategory,
      productBrand,
      {
        description: productRevision?.description,
        sourceProductKind: productRevision?.source_product_kind,
        media: productMedia,
        attributes: productAttributes,
      },
    ),
    revisionNo: productSourceRevisionNo,
  }

  const item: SupplierCatalogItemView = {
    ...projectedItem,
    supplierProduct: {
      ...projectedItem.supplierProduct,
      currentRevision,
      catalogSkus:
        catalogSkus.length > 0
          ? catalogSkus
          : projectedItem.supplierProduct.catalogSkus,
    },
  }

  return {
    item,
    section: input.section ?? "overview",
    related: {
      publications: [],
      historyOrders: [],
      techExceptions:
        item.changeType === "ERROR"
          ? [
              {
                id: "te1",
                label: "接口错误与对账",
                href: `/governance/integration-errors?from=W21&supplierCatalogSkuId=${encodeURIComponent(
                  item.supplierProduct.catalogSkus?.[0]?.id ??
                    `${item.supplierProduct.id}_sku`
                )}`,
              },
            ]
          : [],
    },
  }
}

export function getSessionDraft(
  supplierCatalogSkuId: string
): SessionCatalogDraft | null {
  return drafts.get(supplierCatalogSkuId) ?? null
}

export async function saveSessionDraft(input: {
  supplierCatalogSkuId: string
  selectedSkuId?: string
  offeringDraft?: SessionCatalogDraft["offeringDraft"]
  substituteCandidateSkuIds?: string[]
  note?: string
}): Promise<SessionCatalogDraft> {
  const next: SessionCatalogDraft = {
    supplierCatalogSkuId: input.supplierCatalogSkuId,
    selectedSkuId: input.selectedSkuId,
    offeringDraft: input.offeringDraft,
    substituteCandidateSkuIds: input.substituteCandidateSkuIds,
    note: input.note,
    updatedAt: new Date().toISOString(),
  }
  drafts.set(input.supplierCatalogSkuId, next)
  return next
}

export async function claimSupplierCatalogWorkItem(
  workItemId: string
): Promise<WorkItemLease> {
  // 合成 ID 表示后端尚未为该目录条目派发 work_item
  if (workItemId.startsWith("wi_catalog_")) {
    return {
      workItemId,
      claimedByLabel: "当前用户",
    }
  }
  const detail = await apiGet<BackendWorkItem>(
    `/admin/work-items/${encodeURIComponent(workItemId)}`
  )
  await apiPost(`/admin/work-items/${encodeURIComponent(workItemId)}/claim`, {
    version: detail.version,
  })
  return {
    workItemId,
    claimedByLabel: detail.owner_user_id ?? "当前用户",
  }
}

export async function applySupplierCatalogWorkItemAction(input: {
  workItemId: string
  action: SupplierCatalogWorkItemAction
  expectedSubjectVersion?: string
}): Promise<FormalActionResponse> {
  if (input.workItemId.startsWith("wi_catalog_")) {
    return {
      status: "failed",
      code: "BACKEND_GAP",
      message:
        "该目录条目尚未注册为 work_item；无法执行任务内动作。请先由异常派发流程创建待办。",
    }
  }

  const detail = await apiGet<BackendWorkItem>(
    `/admin/work-items/${encodeURIComponent(input.workItemId)}`
  )

  if (input.action.kind === "HOLD") {
    await apiPost(
      `/admin/work-items/${encodeURIComponent(input.workItemId)}/defer`,
      {
        version: detail.version,
        comment: input.action.comment ?? input.action.reasonCode,
      }
    )
    return {
      status: "succeeded",
      outcome: {
        kind: "ACTION",
        workItemId: input.workItemId,
        workItemStatus: "IN_PROGRESS",
        actionKind: "HOLD",
        heldAt: new Date().toISOString(),
        resumeHint: "暂挂后可在统一任务队列恢复处理",
        reference: input.workItemId,
      },
    }
  }

  if (input.action.kind === "RETURN_FOR_DATA_FIX") {
    await apiPost(
      `/admin/work-items/${encodeURIComponent(input.workItemId)}/transfer`,
      {
        version: detail.version,
        owner_role: input.action.suggestedResponsibleRole ?? "ops",
        owner_user_id: "unassigned",
        comment: input.action.comment ?? input.action.reasonCode,
      }
    )
    return {
      status: "succeeded",
      outcome: {
        kind: "ACTION",
        workItemId: input.workItemId,
        workItemStatus: "PENDING",
        actionKind: "RETURN_FOR_DATA_FIX",
        resumeHint: "已退回数据修复",
        reference: input.workItemId,
      },
    }
  }

  return {
    status: "succeeded",
    outcome: {
      kind: "ACTION",
      workItemId: input.workItemId,
      workItemStatus: "IN_PROGRESS",
      actionKind: input.action.kind,
      resumeHint: "动作已记录",
      reference: input.workItemId,
    },
  }
}

export async function completeSupplierCatalogWorkItem(input: {
  workItemId: string
  decision: SupplierCatalogDecision
  expectedSubjectVersion?: string
}): Promise<FormalActionResponse> {
  if (input.workItemId.startsWith("wi_catalog_")) {
    return {
      status: "failed",
      code: "BACKEND_GAP",
      message:
        "该目录条目尚未注册为 work_item；完成动作不可用。异常/停供确认需先有正式待办。",
    }
  }

  const detail = await apiGet<BackendWorkItem>(
    `/admin/work-items/${encodeURIComponent(input.workItemId)}`
  )
  await apiPost(
    `/admin/work-items/${encodeURIComponent(input.workItemId)}/complete`,
    { version: detail.version }
  )

  return {
    status: "succeeded",
    outcome: {
      kind: "COMPLETED",
      business: {
        decisionKind: input.decision.kind,
        supplierProductId: detail.business_object_id,
        supplierCatalogSkuId: detail.business_object_id,
        auditEventId: `complete_${input.workItemId}`,
        publicationImpact: publicationImpact(),
        reference: input.workItemId,
        completedAt: new Date().toISOString(),
        subjectHash: detail.subject_version ?? detail.id,
      },
    },
  }
}

export async function createSupplierCatalogItem(
  input: CreateSupplierCatalogItemInput
): Promise<SupplierCatalogWriteResult> {
  const skus =
    input.skus && input.skus.length > 0
      ? input.skus.map((s) => {
          const mainImage = skuMainImageFromWrite(s)
          return {
            supplier_sku_code: s.supplierSkuCode,
            name: input.name,
            specification: s.specification ?? input.specification,
            source_base_unit: input.sourceBaseUnit ?? input.baseUnit ?? null,
            barcode: s.barcode ?? null,
            source_main_image_url: mainImage.url,
            ...(mainImage.assetId
              ? { source_main_image_asset_id: mainImage.assetId }
              : {}),
            dropship_floor_price_gross: s.dropshipFloorPriceGross || null,
            bulk_floor_price_gross: s.bulkFloorPriceGross || null,
            bulk_minimum_order_quantity: s.bulkMinimumOrderQuantity || null,
            available_quantity: s.availableQuantity ?? null,
            availability_status: s.availabilityStatus ?? "AVAILABLE",
            structured_attributes: (s.attributes ?? []).map((a) => ({
              attribute_name: a.name,
              attribute_value: a.value,
            })),
          }
        })
      : [
          {
            supplier_sku_code:
              input.supplierSkuCode ??
              input.supplierSpuCode ??
              `SKU-${Date.now()}`,
            name: input.name,
            specification: input.specification,
            source_base_unit: input.sourceBaseUnit ?? input.baseUnit ?? null,
            barcode: input.barcode ?? null,
            ...(() => {
              const mainImage = skuMainImageFromWrite({
                media: input.media,
                mainImage: undefined,
              })
              return {
                source_main_image_url: mainImage.url,
                ...(mainImage.assetId
                  ? { source_main_image_asset_id: mainImage.assetId }
                  : {}),
              }
            })(),
            dropship_floor_price_gross:
              input.dropshipFloorPriceGross || null,
            bulk_floor_price_gross: input.bulkFloorPriceGross || null,
            bulk_minimum_order_quantity:
              input.bulkMinimumOrderQuantity || null,
            available_quantity: input.availableQuantity ?? null,
            availability_status: input.availabilityStatus ?? "AVAILABLE",
            structured_attributes: (input.attributes ?? []).map((a) => ({
              attribute_name: a.name,
              attribute_value: a.value,
            })),
          },
        ]

  const result = await apiPost<{
    product_id: string
    sku_ids: string[]
    intake_batch_id: string
    intake_item_id: string
    mapping_id?: string | null
    offering_id?: string | null
    offering_revision_id?: string | null
    replayed: boolean
    reference: string
    recorded_at: number
  }>("/admin/supplier-catalog/products", {
    source_type: input.sourceType,
    supplier_id: input.supplierId,
    source_reference: input.sourceReference ?? input.idempotencyKey,
    supplier_spu_code:
      input.supplierSpuCode ?? input.supplierSkuCode ?? `SPU-${Date.now()}`,
    name: input.name,
    description: input.description ?? null,
    source_product_kind: input.sourceProductKind ?? null,
    source_category: input.category || null,
    source_brand: input.brand ?? null,
    structured_attributes: (input.attributes ?? []).map((a) => ({
      attribute_name: a.name,
      attribute_value: a.value,
    })),
    media: mediaToWritePayload(input.media ?? []),
    source_revision_token: null,
    valid_from: input.validFrom || null,
    valid_to: null,
    skus,
    target_supply: input.targetSkuId
      ? {
          sku_id: input.targetSkuId,
          input_tax_rate: input.inputTaxRate,
          supply_region: input.supplyRegion ?? [],
        }
      : null,
    idempotency_key: input.idempotencyKey,
  })

  return {
    supplierProductId: result.product_id,
    supplierCatalogSkuId: result.sku_ids[0],
    supplierOfferingRevisionId: result.offering_revision_id ?? undefined,
    poolEntryChange: result.mapping_id ? "CREATED" : "NONE",
    reference: result.reference,
    recordedAt: secsToIso(result.recorded_at),
  }
}

/** 将浏览器预检结果作为一个正式 Excel 批次原子写入。 */
export async function importSupplierCatalogExcel(
  input: SupplierCatalogExcelImportInput
): Promise<SupplierCatalogExcelImportResult> {
  const result = await apiPost<{
    intake_batch_id: string
    product_ids: string[]
    imported_count: number
    rejected_count: number
    replayed: boolean
    reference: string
    recorded_at: number
  }>("/admin/supplier-catalog/imports/excel", {
    supplier_id: input.supplierId,
    source_reference: input.sourceReference,
    file_asset_id: input.fileAssetId,
    products: input.preview.products.map((product) => ({
      supplier_spu_code: product.supplierSpuCode,
      name: product.name,
      description: product.description ?? null,
      source_product_kind: product.sourceProductKind ?? null,
      source_category: product.sourceCategory ?? null,
      source_brand: product.sourceBrand ?? null,
      structured_attributes: product.attributes.map((attribute) => ({
        attribute_name: attribute.name,
        attribute_value: attribute.value,
      })),
      media: product.media
        .filter((media) => Boolean(media.sourceUrl))
        .map((media) => ({
          usage: media.usage,
          url: media.sourceUrl,
          file_asset_id: media.fileAssetId ?? null,
        })),
      source_revision_token: product.sourceRevisionToken ?? null,
      valid_from: product.validFrom ?? null,
      valid_to: product.validTo ?? null,
      skus: product.skus.map((row) => ({
        row_no: row.rowNo,
        sku: {
          supplier_catalog_sku_id: null,
          supplier_sku_code: row.supplierSkuCode,
          name: row.name,
          specification: row.specification,
          source_base_unit: row.sourceBaseUnit ?? null,
          barcode: row.barcode ?? null,
          source_main_image_url: row.mainImageUrl ?? null,
          source_main_image_asset_id: null,
          dropship_floor_price_gross: row.dropshipFloorPriceGross ?? null,
          bulk_floor_price_gross: row.bulkFloorPriceGross ?? null,
          bulk_minimum_order_quantity: row.bulkMinimumOrderQuantity ?? null,
          available_quantity: row.availableQuantity ?? null,
          availability_status: row.availabilityStatus,
          structured_attributes: row.attributes.map((attribute) => ({
            attribute_name: attribute.name,
            attribute_value: attribute.value,
          })),
        },
      })),
    })),
    rejected_rows: input.preview.rejectedRows.map((row) => ({
      row_no: row.rowNo,
      supplier_sku_code: row.supplierSkuCode,
      error_text: row.errorText,
    })),
    idempotency_key: input.idempotencyKey,
  })
  return {
    intakeBatchId: result.intake_batch_id,
    productIds: result.product_ids,
    importedCount: result.imported_count,
    rejectedCount: result.rejected_count,
    replayed: result.replayed,
    reference: result.reference,
    recordedAt: secsToIso(result.recorded_at),
  }
}

export async function reviseSupplierCatalogProduct(
  input: ReviseSupplierCatalogProductInput
): Promise<SupplierCatalogWriteResult> {
  const result = await apiPost<{
    product_id: string
    revision_no: number
    reference: string
    recorded_at: number
  }>(
    `/admin/supplier-catalog/products/${encodeURIComponent(input.supplierProductId)}/revisions`,
    {
      expected_revision_no: input.expectedSourceRevisionNo,
      supplier_spu_code: input.supplierSpuCode ?? "",
      name: input.name,
      description: input.description ?? null,
      source_product_kind: input.sourceProductKind ?? null,
      source_category: input.category || null,
      source_brand: input.brand ?? null,
      structured_attributes: (input.attributes ?? []).map((a) => ({
        attribute_name: a.name,
        attribute_value: a.value,
      })),
      media: mediaToWritePayload(input.media ?? []),
      source_revision_token: null,
      valid_from: null,
      valid_to: null,
      skus: input.skus.map((s) => {
        const mainImage = skuMainImageFromWrite(s)
        return {
          supplier_catalog_sku_id: s.id ?? null,
          supplier_sku_code: s.supplierSkuCode,
          name: input.name,
          specification: s.specification ?? input.specification,
          source_base_unit: input.sourceBaseUnit ?? null,
          barcode: s.barcode ?? null,
          source_main_image_url: mainImage.url,
          ...(mainImage.assetId
            ? { source_main_image_asset_id: mainImage.assetId }
            : {}),
          dropship_floor_price_gross: s.dropshipFloorPriceGross || null,
          bulk_floor_price_gross: s.bulkFloorPriceGross || null,
          bulk_minimum_order_quantity: s.bulkMinimumOrderQuantity || null,
          available_quantity: s.availableQuantity ?? null,
          availability_status: s.availabilityStatus ?? "AVAILABLE",
          structured_attributes: (s.attributes ?? []).map((a) => ({
            attribute_name: a.name,
            attribute_value: a.value,
          })),
        }
      }),
      change_reason: input.changeReason,
      idempotency_key: input.idempotencyKey,
    }
  )

  return {
    supplierProductId: result.product_id,
    poolEntryChange: "NONE",
    reference: result.reference,
    recordedAt: secsToIso(result.recorded_at),
  }
}

/** 为既有供给追加不可变修订，并由后端执行资质门禁与幂等校验。 */
export async function reviseSupplierOffering(input: ReviseSupplierOfferingInput) {
  return apiPost<{
    offering_id: string
    revision_no: number
    status: "ACTIVE" | "PAUSED" | "STOPPED"
    version: number
    reference: string
  }>(
    `/admin/supplier-catalog/offerings/${encodeURIComponent(input.offeringId)}/revisions`,
    {
      expected_revision_no: input.expectedRevisionNo,
      dropship_supply_price_gross: input.dropshipSupplyPriceGross,
      bulk_supply_price_gross: input.bulkSupplyPriceGross,
      input_tax_rate: input.inputTaxRate,
      bulk_minimum_order_quantity: input.bulkMinimumOrderQuantity,
      supply_region: input.supplyRegion,
      product_capabilities: input.productCapabilities,
      valid_from: input.validFrom,
      valid_to: input.validTo ?? null,
      dropship_express: input.dropshipExpress ?? null,
      freight_amount: input.freightAmount ?? null,
      service_fee_amount: input.serviceFeeAmount ?? null,
      available_quantity: input.availableQuantity ?? null,
      status: input.status,
      change_reason: input.changeReason,
      idempotency_key: input.idempotencyKey,
    }
  )
}

export async function promoteSupplierProductToPool(
  input: PromoteSupplierProductInput
): Promise<SupplierCatalogWriteResult> {
  const created = await apiPost<{
    mapping_id: string
    status: string
    version: number
    reference: string
  }>("/admin/supplier-catalog/mappings", {
    supplier_catalog_sku_id: input.supplierCatalogSkuId,
    sku_id: input.targetSkuId,
    reason: "promote_to_pool",
  })

  const approved = await apiPost<{
    mapping_id: string
    status: string
    offering_id: string
    offering_revision_no: number
    version: number
    reference: string
  }>(
    `/admin/supplier-catalog/mappings/${encodeURIComponent(created.mapping_id)}/approve`,
    {
      expected_version: created.version,
      dropship_supply_price_gross: input.confirmedCostGross,
      bulk_supply_price_gross: input.confirmedCostGross,
      input_tax_rate: input.inputTaxRate,
      bulk_minimum_order_quantity: input.minimumOrderQuantity,
      supply_region: input.supplyRegion,
      valid_from: input.validFrom,
      valid_to: null,
      dropship_express: null,
      freight_amount: null,
      service_fee_amount: null,
      available_quantity: null,
    }
  )

  return {
    supplierProductId: input.supplierCatalogSkuId,
    supplierCatalogSkuId: input.supplierCatalogSkuId,
    companySkuId: input.targetSkuId,
    productKind: input.productKind,
    supplierOfferingRevisionId: `${approved.offering_id}:r${approved.offering_revision_no}`,
    poolEntryChange: "CREATED",
    reference: approved.reference,
    recordedAt: new Date().toISOString(),
  }
}

export async function fetchSupplierProductPoolMatch(
  supplierProductId: string
): Promise<SupplierProductPoolMatchView> {
  const result = await apiGet<{
    supplier_product_id: string
    source_revision_no: number
    items: Array<{
      supplier_catalog_sku_id: string
      supplier_sku_code: string
      specification?: string | null
      barcode?: string | null
      pool_status: "MAPPED" | "HAS_CANDIDATES" | "UNMATCHED"
      mapped_company_sku_id?: string | null
      mapped_company_sku_no?: string | null
      candidates: Array<{
        sku_id: string
        sku_no: string
        product_id: string
        product_no: string
        name: string
        specification?: string | null
        barcode?: string | null
        base_unit_id: string
        sales_visible_price_gross?: string | null
        active_supplier_count: number
        match_signals: string[]
        score: number
      }>
    }>
  }>(
    `/admin/supplier-catalog/products/${encodeURIComponent(supplierProductId)}/pool-match`
  )

  return {
    supplierProductId: result.supplier_product_id,
    sourceRevisionNo: result.source_revision_no,
    items: result.items.map((item) => ({
      supplierCatalogSkuId: item.supplier_catalog_sku_id,
      supplierSkuCode: item.supplier_sku_code,
      specification: item.specification ?? undefined,
      barcode: item.barcode ?? undefined,
      poolStatus: item.pool_status,
      mappedCompanySkuId: item.mapped_company_sku_id ?? undefined,
      mappedCompanySkuNo: item.mapped_company_sku_no ?? undefined,
      candidates: (item.candidates ?? []).map((c) => ({
        skuId: c.sku_id,
        skuNo: c.sku_no,
        productId: c.product_id,
        productNo: c.product_no,
        name: c.name,
        specification: c.specification ?? undefined,
        barcode: c.barcode ?? undefined,
        baseUnitId: c.base_unit_id,
        salesVisiblePriceGross: c.sales_visible_price_gross ?? undefined,
        activeSupplierCount: c.active_supplier_count,
        matchSignals: c.match_signals ?? [],
        score: c.score,
      })),
    })),
  }
}

export async function linkPromoteToCompanyPool(
  input: LinkPromoteToCompanyPoolInput
): Promise<SupplierCatalogWriteResult> {
  const result = await apiPost<{
    supplier_product_id: string
    items: Array<{
      supplier_catalog_sku_id: string
      company_sku_id: string
      mapping_id: string
      offering_id: string
      offering_revision_no: number
    }>
    reference: string
    recorded_at: number
  }>("/admin/supplier-catalog/link-promote", {
    supplier_product_id: input.supplierProductId,
    expected_source_revision_no: input.expectedSourceRevisionNo,
    input_tax_rate: input.inputTaxRate,
    supply_region: input.supplyRegion,
    items: input.items.map((item) => ({
      supplier_catalog_sku_id: item.supplierCatalogSkuId,
      company_sku_id: item.companySkuId,
      dropship_supply_price_gross: item.dropshipSupplyPriceGross ?? null,
      bulk_supply_price_gross: item.bulkSupplyPriceGross ?? null,
    })),
    idempotency_key: input.idempotencyKey,
  })

  const first = result.items[0]
  return {
    supplierProductId: result.supplier_product_id,
    supplierCatalogSkuId: first?.supplier_catalog_sku_id,
    companySkuId: first?.company_sku_id,
    supplierOfferingRevisionId: first
      ? `${first.offering_id}:r${first.offering_revision_no}`
      : undefined,
    poolEntryChange: "UNCHANGED",
    activeSupplierCount: result.items.length,
    reference: result.reference,
    recordedAt: new Date(result.recorded_at * 1000).toISOString(),
  }
}

export async function reversePromoteToCompanyPool(
  input: ReversePromoteToCompanyPoolInput
): Promise<SupplierCatalogWriteResult> {
  const result = await apiPost<{
    supplier_product_id: string
    company_product_id: string
    product_no: string
    product_kind: string
    items: Array<{
      supplier_catalog_sku_id: string
      company_sku_id: string
      company_sku_revision_id: string
      mapping_id: string
      offering_id: string
      offering_revision_no: number
    }>
    reference: string
    recorded_at: number
  }>("/admin/supplier-catalog/reverse-promote", {
    supplier_product_id: input.supplierProductId,
    expected_source_revision_no: input.expectedSourceRevisionNo,
    product_kind: input.productKind,
    product_no: input.productNo ?? null,
    category_id: input.categoryId,
    brand_id: input.brandId,
    base_unit_id: input.baseUnitId,
    input_tax_rate: input.inputTaxRate,
    supply_region: input.supplyRegion,
    items: input.items.map((item) => ({
      supplier_catalog_sku_id: item.supplierCatalogSkuId,
      sku_no: item.skuNo ?? null,
      dropship_supply_price_gross: item.dropshipSupplyPriceGross ?? null,
      bulk_supply_price_gross: item.bulkSupplyPriceGross ?? null,
      sales_visible_price_gross: item.salesVisiblePriceGross,
      market_price: item.marketPrice,
    })),
    idempotency_key: input.idempotencyKey,
  })

  const first = result.items[0]
  return {
    supplierProductId: result.supplier_product_id,
    supplierCatalogSkuId: first?.supplier_catalog_sku_id,
    companyProductId: result.company_product_id,
    companySkuId: first?.company_sku_id,
    productKind: result.product_kind,
    supplierOfferingRevisionId: first
      ? `${first.offering_id}:r${first.offering_revision_no}`
      : undefined,
    poolEntryChange: "CREATED",
    reference: result.reference,
    recordedAt: new Date(result.recorded_at * 1000).toISOString(),
  }
}

/** @deprecated 使用 reversePromoteToCompanyPool */
export async function createCompanyProductFromSupplierSku(
  input: CreateCompanyProductFromSupplierSkuInput
): Promise<SupplierCatalogWriteResult> {
  return reversePromoteToCompanyPool(input)
}

export async function attemptUnregisteredFormalWrite(): Promise<FormalActionResponse> {
  return {
    status: "failed",
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message: REGISTRATION_BLOCKER_MESSAGE,
  }
}
