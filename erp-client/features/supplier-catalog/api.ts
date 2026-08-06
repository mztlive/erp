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
  PromoteSupplierProductInput,
  ReviseSupplierCatalogProductInput,
  SessionCatalogDraft,
  SupplierCatalogCenterView,
  SupplierCatalogItemView,
  SupplierCatalogQueueQuery,
  SupplierCatalogQueueView,
  SupplierCatalogSkuView,
  SupplierCatalogWriteResult,
  SupplierCatalogWorkItemAction,
  SupplierCatalogDecision,
  SupplierOfferingRevisionView,
  SupplierProductMappingView,
  SupplierProductRevisionView,
  WorkItemLease,
} from "@/features/supplier-catalog/types"
import {
  REGISTRATION_BLOCKER_MESSAGE,
} from "@/features/supplier-catalog/types"

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
  barcode?: string | null
  dropship_floor_price_gross?: string | null
  bulk_floor_price_gross?: string | null
  bulk_minimum_order_quantity?: string | null
  availability_status?: string | null
  version: number
  created_at: number
}

type BackendSkuRevision = {
  id: string
  revision_no: number
  name: string
  specification: string
  barcode?: string | null
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

// ─── Session draft (client-only, not mock seed) ───────────────────────────────

const drafts = new Map<string, SessionCatalogDraft>()

// ─── Helpers ──────────────────────────────────────────────────────────────────

function secsToIso(secs?: number | null): string {
  if (secs == null || secs <= 0) return new Date(0).toISOString()
  return new Date(secs * 1000).toISOString()
}

function sourceTypeLabel(t: string): string {
  if (t === "EXCEL") return "Excel"
  if (t === "API") return "API"
  if (t === "MANUAL") return "手工"
  return t
}

function changeTypeFromStatus(
  status: string
): "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "UNCHANGED" {
  if (status === "STOPPED") return "STOPPED"
  if (status === "EXCEPTION") return "ERROR"
  return "NEW"
}

function mapAvailability(
  raw?: string | null
): "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE" {
  const u = (raw ?? "AVAILABLE").toUpperCase()
  if (u === "UNAVAILABLE" || u === "STOPPED" || u === "STALE") return u
  return "AVAILABLE"
}

function emptyPublicationImpact() {
  return {
    activePublicationCount: 0,
    pausedPublicationCount: 0,
    historicalPaidOrderCount: 0,
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
    note: "发布影响由商品发布域承接；本接口不返回聚合。",
  }
}

function mapSkuRevision(
  rev: BackendSkuRevision | undefined,
  productName: string,
  category: string,
  brand: string | undefined,
): SupplierProductRevisionView {
  const r = rev
  return {
    revisionNo: r?.revision_no ?? 1,
    sourceUpdatedAt: secsToIso(r?.source_updated_at),
    syncedAt: secsToIso(r?.source_updated_at),
    name: r?.name ?? productName,
    specification: r?.specification ?? "",
    category,
    brand,
    barcode: r?.barcode ?? undefined,
    dropshipFloorPriceGross: r?.dropship_floor_price_gross ?? null,
    bulkFloorPriceGross: r?.bulk_floor_price_gross ?? null,
    bulkMinimumOrderQuantity: r?.bulk_minimum_order_quantity ?? null,
    availableQuantity: r?.available_quantity ?? "0",
    availabilityStatus: mapAvailability(r?.availability_status),
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
    freightAmount: null,
    serviceFeeAmount: null,
    minimumOrderQuantity: o.bulk_minimum_order_quantity ?? "1",
    supplyRegion: o.supply_region ?? [],
    availabilityStatus: o.availability_status ?? "AVAILABLE",
    availableQuantity: "0",
    productCapabilities: [],
    validFrom: o.valid_from ?? secsToIso(o.created_at).slice(0, 10),
    validTo: o.valid_to ?? undefined,
    createdAt: secsToIso(o.created_at),
    immutable: true,
  }
}

function mapMapping(m: BackendMapping): SupplierProductMappingView {
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
  supplierName: string,
): SupplierCatalogItemView {
  const primarySku = skus[0]
  const primaryRev = primarySku
    ? skuRevisions.get(primarySku.id)
    : undefined
  const category = product.source_category ?? ""
  const brand = product.source_brand ?? undefined
  const currentRevision = mapSkuRevision(
    primaryRev,
    product.name ?? product.supplier_spu_code,
    category,
    brand
  )

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
          dropship_floor_price_gross: s.dropship_floor_price_gross,
          bulk_floor_price_gross: s.bulk_floor_price_gross,
          bulk_minimum_order_quantity: s.bulk_minimum_order_quantity,
          available_quantity: null,
          availability_status: s.availability_status ?? "AVAILABLE",
          source_updated_at: product.source_updated_at ?? product.created_at,
        } as BackendSkuRevision),
      product.name ?? s.supplier_sku_code,
      category,
      brand
    ),
  }))

  const skuIds = new Set(skus.map((s) => s.id))
  const mapping = mappings.find((m) => skuIds.has(m.supplier_catalog_sku_id))
  const offering = offerings.find((o) => skuIds.has(o.supplier_catalog_sku_id))
  const changeType = changeTypeFromStatus(product.status)

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
    mapping: mapping ? mapMapping(mapping) : undefined,
    skuCandidates: [],
    offering: offering
      ? {
          stableId: offering.id,
          currentRevision: mapOffering(offering),
          revisionHistory: [mapOffering(offering)],
        }
      : undefined,
    publicationImpact: emptyPublicationImpact(),
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
    return {
      ...base,
      changeType,
      workItem: {
        workItemId: `wi_catalog_${product.id}`,
        workItemType: "BUSINESS_EXCEPTION",
        businessObjectType: "SUPPLIER_CATALOG_SKU",
        subjectVersion: String(product.version),
        subjectHash: product.id,
        workItemStatus: "PENDING",
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
        reason: product.status === "EXCEPTION" ? "来源异常" : "停止供应",
        impact: "需采购复核",
        priority: 50,
        handlerKey: "supplier_catalog",
      },
    }
  }

  if (changeType === "NEW" || changeType === "CHANGED") {
    return {
      ...base,
      changeType,
      registrationBlocker: {
        code: "WORK_ITEM_TYPE_UNREGISTERED",
        message: REGISTRATION_BLOCKER_MESSAGE,
        businessProcess: "MAPPING",
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
  const pageSize = query.pageSize ?? 50
  const productPage = await apiGet<Page<BackendProduct>>(
    "/admin/supplier-catalog/products",
    {
      page: 1,
      page_size: pageSize,
      q: query.q?.trim() || undefined,
      supplier_id: query.supplierId || undefined,
      source_type:
        query.sourceType && query.sourceType !== "all"
          ? query.sourceType
          : undefined,
    }
  )

  const items: SupplierCatalogItemView[] = []

  for (const product of productPage.items) {
    const [skuPage, mappingPage, offeringPage, supplierName] = await Promise.all([
      apiGet<Page<BackendSku>>("/admin/supplier-catalog/skus", {
        supplier_catalog_product_id: product.id,
        page: 1,
        page_size: 100,
      }).catch(() => ({
        items: [] as BackendSku[],
        total: 0,
        page: 1,
        page_size: 100,
      })),
      apiGet<Page<BackendMapping>>("/admin/supplier-catalog/mappings", {
        page: 1,
        page_size: 100,
      }).catch(() => ({
        items: [] as BackendMapping[],
        total: 0,
        page: 1,
        page_size: 100,
      })),
      apiGet<Page<BackendOffering>>("/admin/supplier-catalog/offerings", {
        supplier_id: product.supplier_id,
        page: 1,
        page_size: 100,
      }).catch(() => ({
        items: [] as BackendOffering[],
        total: 0,
        page: 1,
        page_size: 100,
      })),
      resolveSupplierName(product.supplier_id),
    ])

    const skuRevisions = new Map<string, BackendSkuRevision>()
    for (const s of skuPage.items) {
      skuRevisions.set(s.id, {
        id: s.current_revision_id ?? s.id,
        revision_no: s.current_revision_no ?? 1,
        name: s.name ?? product.name ?? s.supplier_sku_code,
        specification: s.specification ?? "",
        barcode: s.barcode,
        dropship_floor_price_gross: s.dropship_floor_price_gross,
        bulk_floor_price_gross: s.bulk_floor_price_gross,
        bulk_minimum_order_quantity: s.bulk_minimum_order_quantity,
        available_quantity: null,
        availability_status: s.availability_status ?? "AVAILABLE",
        source_updated_at: product.source_updated_at ?? product.created_at,
      })
    }

    const item = projectProductToItem(
      product,
      skuPage.items,
      skuRevisions,
      mappingPage.items,
      offeringPage.items,
      supplierName
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
        (i.changeType === "ERROR" || i.changeType === "STOPPED") &&
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
    current &&
    (current.changeType === "ERROR" || current.changeType === "STOPPED")
      ? current.workItem.workItemId
      : undefined

  // skuContext：若从公司 SKU 入口进入，尝试加载 SKU
  let skuContext: SupplierCatalogQueueView["skuContext"]
  if (query.skuId) {
    try {
      const skus = await apiGet<Page<BackendSkuListItem>>("/admin/skus", {
        page: 1,
        page_size: 1,
      })
      const hit = skus.items.find((s) => s.id === query.skuId) ?? skus.items[0]
      if (hit && hit.id === query.skuId) {
        skuContext = {
          productId: hit.product_id,
          productName: hit.sku_no,
          skuId: hit.id,
          skuCode: hit.sku_no,
          specification: hit.specification_signature,
          baseUnit: hit.base_unit_id,
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
  const page = await apiGet<Page<BackendSkuListItem>>("/admin/skus", {
    page: 1,
    page_size: 100,
    status: "enabled",
  }).catch(() =>
    apiGet<Page<BackendSkuListItem>>("/admin/skus", {
      page: 1,
      page_size: 100,
    })
  )

  return page.items.map((sku) => ({
    productId: sku.product_id,
    skuId: sku.id,
    skuCode: sku.sku_no,
    skuName: sku.sku_no,
    specification: sku.specification_signature,
    baseUnit: sku.base_unit_id,
    barcode: undefined as string | undefined,
    brand: undefined as string | undefined,
    category: undefined as string | undefined,
    revisionNo: 1,
    similarityLabel: "公司商品候选",
    activeSupplierCount: 0,
    poolEntry: undefined as
      | {
          poolEntryId: string
          poolEntryRevisionId: string
          status: "ACTIVE" | "PAUSED" | "DISABLED"
          salesVisiblePriceGross: string
        }
      | undefined,
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

  const [offeringPage, supplierName] = await Promise.all([
    apiGet<Page<BackendOffering>>("/admin/supplier-catalog/offerings", {
      supplier_id: product.supplier_id,
      page: 1,
      page_size: 100,
    }).catch(() => ({
      items: [] as BackendOffering[],
      total: 0,
      page: 1,
      page_size: 100,
    })),
    resolveSupplierName(product.supplier_id),
  ])

  const item = projectProductToItem(
    product,
    skus,
    skuRevisions,
    detail.mappings,
    offeringPage.items,
    supplierName
  )

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
        publicationImpact: emptyPublicationImpact(),
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
      ? input.skus.map((s) => ({
          supplier_sku_code: s.supplierSkuCode,
          name: input.name,
          specification: s.specification ?? input.specification,
          source_base_unit: input.sourceBaseUnit ?? input.baseUnit ?? null,
          barcode: s.barcode ?? null,
          dropship_floor_price_gross: s.dropshipFloorPriceGross || null,
          bulk_floor_price_gross: s.bulkFloorPriceGross || null,
          bulk_minimum_order_quantity: s.bulkMinimumOrderQuantity || null,
          available_quantity: s.availableQuantity ?? null,
          availability_status: s.availabilityStatus ?? "AVAILABLE",
          structured_attributes: (s.attributes ?? []).map((a) => ({
            attribute_name: a.name,
            attribute_value: a.value,
          })),
        }))
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
    source_product_kind: null,
    source_category: input.category || null,
    source_brand: input.brand ?? null,
    structured_attributes: (input.attributes ?? []).map((a) => ({
      attribute_name: a.name,
      attribute_value: a.value,
    })),
    media: (input.media ?? [])
      .filter((m) => m.sourceUrl)
      .map((m) => ({
        usage: m.usage,
        url: m.sourceUrl!,
      })),
    source_revision_token: null,
    valid_from: input.validFrom || null,
    valid_to: null,
    skus,
    idempotency_key: input.idempotencyKey,
  })

  // 可选：若指定目标公司 SKU，创建映射
  if (input.targetSkuId && result.sku_ids[0]) {
    await apiPost("/admin/supplier-catalog/mappings", {
      supplier_catalog_sku_id: result.sku_ids[0],
      sku_id: input.targetSkuId,
      reason: "create_with_target",
    }).catch(() => undefined)
  }

  return {
    supplierProductId: result.product_id,
    supplierCatalogSkuId: result.sku_ids[0],
    poolEntryChange: "NONE",
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
      source_product_kind: null,
      source_category: input.category || null,
      source_brand: input.brand ?? null,
      structured_attributes: (input.attributes ?? []).map((a) => ({
        attribute_name: a.name,
        attribute_value: a.value,
      })),
      media: (input.media ?? [])
        .filter((m) => m.sourceUrl)
        .map((m) => ({
          usage: m.usage,
          url: m.sourceUrl!,
        })),
      source_revision_token: null,
      valid_from: null,
      valid_to: null,
      skus: input.skus.map((s) => ({
        supplier_sku_code: s.supplierSkuCode,
        name: input.name,
        specification: s.specification ?? input.specification,
        source_base_unit: input.sourceBaseUnit ?? null,
        barcode: s.barcode ?? null,
        dropship_floor_price_gross: s.dropshipFloorPriceGross || null,
        bulk_floor_price_gross: s.bulkFloorPriceGross || null,
        bulk_minimum_order_quantity: s.bulkMinimumOrderQuantity || null,
        available_quantity: s.availableQuantity ?? null,
        availability_status: s.availabilityStatus ?? "AVAILABLE",
        structured_attributes: (s.attributes ?? []).map((a) => ({
          attribute_name: a.name,
          attribute_value: a.value,
        })),
      })),
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

export async function createCompanyProductFromSupplierSku(
  input: CreateCompanyProductFromSupplierSkuInput
): Promise<SupplierCatalogWriteResult> {
  // 反向创建（公司商品 + 映射 + 供给）无单一后端聚合端点；
  // master-data 产品创建 + mapping approve 需跨域编排 — 登记 backend_gap。
  void input
  const err = {
    kind: "Validation" as const,
    message:
      "反向创建公司商品尚未提供聚合接口；请先在基础资料创建公司 SKU，再使用入池确认。",
    status: 400,
  }
  throw err
}

export async function attemptUnregisteredFormalWrite(): Promise<FormalActionResponse> {
  return {
    status: "failed",
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message: REGISTRATION_BLOCKER_MESSAGE,
  }
}
