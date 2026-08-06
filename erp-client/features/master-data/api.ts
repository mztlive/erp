/**
 * W14 基础资料 · 真实 HTTP 适配层。
 *
 * 保持 queries.ts 对外契约（函数签名 / 返回类型）稳定；后端 Page{items,total,page,page_size}
 * 与域 DTO 在本文件内映射为 MasterData* 视图类型。
 *
 * 后端域：catalog / warehouse / supplier / party（路径均在 /admin/...）
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type { Page } from "@/lib/api/paging"
import {
  WAREHOUSE_WRITE_CODE,
  WAREHOUSE_WRITE_MESSAGE,
  computeMetrics,
  resourceLabel,
} from "@/features/master-data/data"
import type {
  BrandFields,
  CategoryFields,
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  LifecycleStatus,
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataListQuery,
  MasterDataListResult,
  MasterDataMutationResult,
  MasterDataResource,
  ProductFields,
  ProductKind,
  ProductSkuFields,
  RevisionTimelineEntry,
  SellableItemFields,
  SupplierFields,
  VoucherCategoryFields,
} from "@/features/master-data/types"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"

// ---------------------------------------------------------------------------
// Backend DTO shapes (subset used by this feature)
// ---------------------------------------------------------------------------

type EnableStatus = "active" | "disabled"

type BackendPage<T> = Page<T>

type ProductCategoryDto = {
  id: string
  category_code: string
  parent_category_id: string | null
  name: string
  product_kind: ProductKind
  status: EnableStatus
  created_at: number
  version: number
}

type ProductBrandDto = {
  id: string
  brand_code: string
  name: string
  status: EnableStatus
  created_at: number
  version: number
}

type ProductDto = {
  id: string
  product_no: string
  product_kind: ProductKind
  status: EnableStatus
  created_at: number
  version: number
}

type ProductRevisionDto = {
  id: string
  product_id: string
  revision_no: number
  name: string
  status: EnableStatus
  effective_from: string
  created_at: number
  version: number
}

type SkuDto = {
  id: string
  sku_no: string
  product_id: string
  base_unit_id: string
  specification_signature: string
  status: EnableStatus
  created_at: number
  version: number
}

type SkuRevisionDto = {
  id: string
  sku_id: string
  revision_no: number
  name: string
  barcode: string | null
  status: EnableStatus
  sales_visible_price_gross: string | null
  effective_from: string
  created_at: number
  version: number
}

type VoucherCategoryProfileDto = {
  id: string
  sku_id: string
  revision_no: number
  description: string
  status: EnableStatus
  created_at: number
  version: number
}

type UnitOfMeasureDto = {
  id: string
  unit_code: string
  name: string
  symbol: string
  quantity_scale: number
  status: EnableStatus
  created_at: number
  version: number
}

type WarehouseDto = {
  id: string
  warehouse_code: string
  status: EnableStatus
  created_at: number
  version: number
}

type WarehouseRevisionDto = {
  id: string
  warehouse_id: string
  revision_no: number
  name: string
  effective_from: string
  effective_to: string | null
  change_reason: string
  created_at: number
  version: number
}

type SupplierDto = {
  id: string
  party_id: string
  supplier_no: string
  default_payment_term_id: string | null
  current_commercial_profile_revision_id: string | null
  status: EnableStatus
  version: number
  created_at: number
}

type CommercialProfileDto = {
  id: string
  supplier_id: string
  revision_no: number
  settlement_mode: string
  reconciliation_cycle: string
  payment_term_snapshot: string
  invoice_type: string
  invoice_tax_rate: string | null
  signing_entity_party_id: string | null
  payment_entity_party_id: string | null
  valid_from: string
  valid_to: string | null
  change_reason: string
  version: number
  created_at: number
}

type SupplierDetailDto = SupplierDto & {
  party_no: string | null
  current_profile: CommercialProfileDto | null
}

type PartyDto = {
  id: string
  party_no: string
  party_kind: string
  unified_credit_code: string | null
  status: string
  current_revision_id: string | null
  version: number
  created_at: number
}

type PartyRevisionDto = {
  id: string
  revision_no: number
  legal_name: string
  short_name: string | null
  effective_from: string
  effective_to: string | null
  change_reason: string
  version: number
  created_at: number
}

type SupplierCapabilityDto = {
  id: string
  supplier_id: string
  capability_code: string
  service_region: string | null
  owner_user_id: string
  fulfillment_note: string | null
  valid_from: string
  valid_to: string | null
  status: EnableStatus
  version: number
  created_at: number
}

type SupplierQualificationDto = {
  id: string
  supplier_id: string
  qualification_type: string
  certificate_no: string
  issuer: string | null
  valid_from: string
  valid_to: string | null
  attachment_id: string | null
  status: string
  version: number
  created_at: number
}

type SupplierRatingDto = {
  id: string
  supplier_id: string
  revision_no: number
  initial_score: number | null
  rating: string
  current_score: number
  valid_from: string
  valid_to: string | null
  change_reason: string
  version: number
  created_at: number
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

const LIST_PAGE_SIZE = 100

const isApiError = (error: unknown): error is ApiError =>
  typeof error === "object" &&
  error !== null &&
  "kind" in error &&
  "message" in error

const asLifecycle = (status: EnableStatus | string): LifecycleStatus =>
  status === "active" || status === "ACTIVE" || status === "ENABLED"
    ? "ENABLED"
    : "DISABLED"

const lifecycleLabel = (status: LifecycleStatus): string =>
  status === "ENABLED" ? "当前启用" : "当前停用"

const lifecycleTone = (
  status: LifecycleStatus
): MasterDataListItem["lifecycleTone"] =>
  status === "ENABLED" ? "success" : "neutral"

const todayDateOnly = (): string => {
  const now = new Date()
  const y = now.getFullYear()
  const m = String(now.getMonth() + 1).padStart(2, "0")
  const d = String(now.getDate()).padStart(2, "0")
  return `${y}-${m}-${d}`
}

const isoNow = (): string => new Date().toISOString()

const tsToIso = (seconds: number | undefined): string => {
  if (!seconds) return isoNow()
  return new Date(seconds * 1000).toISOString()
}

const productKindLabel = (kind: string | undefined): string => {
  if (!kind) return ""
  if (kind in PRODUCT_KIND_LABELS) {
    return PRODUCT_KIND_LABELS[kind as ProductKind]
  }
  // backend OfflineService label
  if (kind === "OFFLINE_SERVICE") return "线下服务"
  return kind
}

const settlementLabel = (mode: string | undefined): string => {
  switch (mode) {
    case "prepayment":
      return "预付款"
    case "pay_after_use":
      return "先用后付"
    case "cash_settlement":
      return "现结"
    default:
      return mode ?? ""
  }
}

const invoiceLabel = (type: string | undefined): string => {
  switch (type) {
    case "vat_special":
      return "增值税专用发票"
    case "vat_normal":
      return "增值税普通发票"
    case "electronic":
      return "电子发票"
    default:
      return type ?? ""
  }
}

const settlementToBackend = (label: string | undefined): string => {
  switch (label) {
    case "预付款":
      return "prepayment"
    case "先用后付":
      return "pay_after_use"
    case "现结":
      return "cash_settlement"
    default:
      return "prepayment"
  }
}

const invoiceToBackend = (label: string | undefined): string => {
  switch (label) {
    case "增值税专用发票":
      return "vat_special"
    case "增值税普通发票":
      return "vat_normal"
    case "电子发票":
      return "electronic"
    default:
      return "vat_normal"
  }
}

const commonActions = (
  resource: MasterDataResource,
  lifecycle: LifecycleStatus
): Pick<MasterDataListItem, "allowedActions" | "actionBlockers"> => {
  if (resource === "warehouses") {
    return {
      allowedActions: ["VIEW", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "CREATE",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
        {
          action: "CREATE_REVISION",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
        {
          action: "DISABLE",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
        {
          action: "MAINTAIN_POLICY",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
      ],
    }
  }
  const allowed: string[] = ["VIEW", "EXPORT_ROW"]
  const blockers: Array<{ action: string; code: string; message: string }> = []
  if (lifecycle === "ENABLED") {
    allowed.push("CREATE_REVISION", "DISABLE")
  } else {
    allowed.push("CREATE_REVISION")
    blockers.push({
      action: "DISABLE",
      code: "ALREADY_DISABLED",
      message: "资料已停用；不是删除，历史记录仍可查看。",
    })
  }
  return { allowedActions: allowed, actionBlockers: blockers }
}

async function fetchAllPages<T>(
  path: string,
  query: Record<string, unknown> = {}
): Promise<T[]> {
  const items: T[] = []
  let page = 1
  let total = Number.POSITIVE_INFINITY
  while (items.length < total) {
    const result = await apiGet<BackendPage<T>>(path, {
      ...query,
      page,
      page_size: LIST_PAGE_SIZE,
    })
    items.push(...result.items)
    total = result.total
    if (result.items.length === 0) break
    page += 1
    if (page > 50) break
  }
  return items
}

function wrapListResult(
  resource: MasterDataResource,
  rows: MasterDataListItem[]
): MasterDataListResult {
  const now = isoNow()
  return {
    resource,
    rows,
    totalCount: rows.length,
    permissionVersion: "pv-w14-http-1",
    effectiveAsOf: now,
    eligibilityAsOf: now,
    queriedAt: now,
    metrics: [...computeMetrics(rows)],
  }
}

function isFutureDate(date: string | undefined): boolean {
  if (!date) return false
  return date > todayDateOnly()
}

// ---------------------------------------------------------------------------
// Resource mappers · list
// ---------------------------------------------------------------------------

function mapCategoryRow(dto: ProductCategoryDto): MasterDataListItem {
  const lifecycle = asLifecycle(dto.status)
  return {
    objectType: "categories",
    stableId: dto.id,
    stableNo: dto.category_code,
    name: dto.name,
    dictionaryCode: dto.category_code,
    parentStableId: dto.parent_category_id ?? undefined,
    productKind: productKindLabel(dto.product_kind),
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: dto.id,
    displayedRevisionId: dto.id,
    revisionNo: dto.version,
    effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
    keyFacts: [
      { label: "分类代码", value: dto.category_code },
      {
        label: "上级分类",
        value: dto.parent_category_id ? dto.parent_category_id : "（根分类）",
      },
      { label: "适用商品类型", value: productKindLabel(dto.product_kind) },
    ],
    selectorEligibility: [],
    ...commonActions("categories", lifecycle),
    lockVersion: dto.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
  }
}

function mapBrandRow(dto: ProductBrandDto): MasterDataListItem {
  const lifecycle = asLifecycle(dto.status)
  return {
    objectType: "brands",
    stableId: dto.id,
    stableNo: dto.brand_code,
    name: dto.name,
    dictionaryCode: dto.brand_code,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: dto.id,
    displayedRevisionId: dto.id,
    revisionNo: dto.version,
    effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
    keyFacts: [{ label: "品牌代码", value: dto.brand_code }],
    selectorEligibility: [],
    ...commonActions("brands", lifecycle),
    lockVersion: dto.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
  }
}

function mapProductRow(
  dto: ProductDto,
  revision?: ProductRevisionDto
): MasterDataListItem {
  const lifecycle = asLifecycle(dto.status)
  const future = revision ? isFutureDate(revision.effective_from) : false
  return {
    objectType: "products",
    stableId: dto.id,
    stableNo: dto.product_no,
    name: revision?.name ?? dto.product_no,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: future ? "FUTURE" : "CURRENT",
    revisionTimingLabel: future ? "待生效" : "当前生效",
    currentRevisionId: revision?.id ?? dto.id,
    displayedRevisionId: revision?.id ?? dto.id,
    revisionNo: revision?.revision_no ?? dto.version,
    effectiveFrom: revision?.effective_from ?? tsToIso(dto.created_at).slice(0, 10),
    keyFacts: [
      { label: "商品编号", value: dto.product_no },
      { label: "商品类型", value: productKindLabel(dto.product_kind) },
    ],
    primaryBlocker: lifecycle === "DISABLED" ? "已停用：历史引用保留" : undefined,
    selectorEligibility: [],
    ...commonActions("products", lifecycle),
    lockVersion: dto.version,
    metricTags: [
      lifecycle === "ENABLED" ? "enabled" : "disabled",
      ...(future ? (["pending"] as const) : []),
    ],
    productKind: productKindLabel(dto.product_kind),
  }
}

function mapSkuAsSellable(
  sku: SkuDto,
  revision?: SkuRevisionDto,
  product?: ProductDto
): MasterDataListItem {
  const lifecycle = asLifecycle(sku.status)
  return {
    objectType: "sellable-items",
    stableId: sku.id,
    stableNo: sku.sku_no,
    name: revision?.name ?? sku.sku_no,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: revision?.id ?? sku.id,
    displayedRevisionId: revision?.id ?? sku.id,
    revisionNo: revision?.revision_no ?? sku.version,
    effectiveFrom:
      revision?.effective_from ?? tsToIso(sku.created_at).slice(0, 10),
    keyFacts: [
      { label: "SKU", value: sku.sku_no },
      {
        label: "销售可见价",
        value: revision?.sales_visible_price_gross
          ? `¥${revision.sales_visible_price_gross}`
          : "—",
      },
      {
        label: "商品编号",
        value: product?.product_no ?? sku.product_id,
      },
    ],
    primaryBlocker:
      lifecycle === "DISABLED" ? "已停用：不可进入销售选品" : undefined,
    selectorEligibility: [],
    ...commonActions("sellable-items", lifecycle),
    lockVersion: sku.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
  }
}

function mapVoucherRow(
  profile: VoucherCategoryProfileDto,
  sku?: SkuDto
): MasterDataListItem {
  const lifecycle = asLifecycle(profile.status)
  return {
    objectType: "voucher-categories",
    stableId: profile.id,
    stableNo: sku?.sku_no ?? profile.sku_id,
    name: profile.description,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: profile.id,
    displayedRevisionId: profile.id,
    revisionNo: profile.revision_no,
    effectiveFrom: tsToIso(profile.created_at).slice(0, 10),
    keyFacts: [
      { label: "卡券 SKU", value: sku?.sku_no ?? profile.sku_id },
      { label: "说明", value: profile.description },
    ],
    primaryBlocker: lifecycle === "DISABLED" ? "已停用" : undefined,
    selectorEligibility: [],
    ...commonActions("voucher-categories", lifecycle),
    lockVersion: profile.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
  }
}

function mapWarehouseRow(
  wh: WarehouseDto,
  revision?: WarehouseRevisionDto
): MasterDataListItem {
  const lifecycle = asLifecycle(wh.status)
  return {
    objectType: "warehouses",
    stableId: wh.id,
    stableNo: wh.warehouse_code,
    name: revision?.name ?? wh.warehouse_code,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: revision?.id ?? wh.id,
    displayedRevisionId: revision?.id ?? wh.id,
    revisionNo: revision?.revision_no ?? wh.version,
    effectiveFrom:
      revision?.effective_from ?? tsToIso(wh.created_at).slice(0, 10),
    effectiveTo: revision?.effective_to ?? undefined,
    keyFacts: [
      { label: "仓库代码", value: wh.warehouse_code },
      ...(revision
        ? [{ label: "变更原因", value: revision.change_reason }]
        : []),
    ],
    primaryBlocker: "暂不可维护（本期）",
    selectorEligibility: [],
    ...commonActions("warehouses", lifecycle),
    lockVersion: wh.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled", "pending"],
  }
}

function mapSupplierRow(
  supplier: SupplierDto,
  partyName?: string,
  profile?: CommercialProfileDto | null
): MasterDataListItem {
  const lifecycle = asLifecycle(supplier.status)
  return {
    objectType: "suppliers",
    stableId: supplier.id,
    stableNo: supplier.supplier_no,
    name: partyName || supplier.supplier_no,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId:
      supplier.current_commercial_profile_revision_id ?? supplier.id,
    displayedRevisionId:
      supplier.current_commercial_profile_revision_id ?? supplier.id,
    revisionNo: profile?.revision_no ?? supplier.version,
    effectiveFrom:
      profile?.valid_from ?? tsToIso(supplier.created_at).slice(0, 10),
    effectiveTo: profile?.valid_to ?? undefined,
    keyFacts: [
      {
        label: "结算方式",
        value: settlementLabel(profile?.settlement_mode) || "—",
      },
      {
        label: "发票类型",
        value: invoiceLabel(profile?.invoice_type) || "—",
      },
    ],
    primaryBlocker: lifecycle === "DISABLED" ? "已停用" : undefined,
    selectorEligibility: [],
    ...commonActions("suppliers", lifecycle),
    lockVersion: supplier.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
  }
}

// ---------------------------------------------------------------------------
// List fetchers
// ---------------------------------------------------------------------------

async function listCategories(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const items = await fetchAllPages<ProductCategoryDto>(
    "/admin/product-categories",
    {
      status,
      name: query.q || undefined,
    }
  )
  // Resolve parent names for keyFacts
  const byId = new Map(items.map((c) => [c.id, c]))
  return items.map((dto) => {
    const row = mapCategoryRow(dto)
    if (dto.parent_category_id) {
      const parent = byId.get(dto.parent_category_id)
      return {
        ...row,
        keyFacts: [
          { label: "分类代码", value: dto.category_code },
          { label: "上级分类", value: parent?.name ?? "（未知上级）" },
          {
            label: "适用商品类型",
            value: productKindLabel(dto.product_kind),
          },
        ],
      }
    }
    return row
  })
}

async function listBrands(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const items = await fetchAllPages<ProductBrandDto>("/admin/product-brands", {
    status,
    name: query.q || undefined,
  })
  return items.map(mapBrandRow)
}

async function listProducts(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const products = await fetchAllPages<ProductDto>("/admin/products", {
    status,
    product_no: query.q || undefined,
  })
  if (products.length === 0) return []

  // Enrich with latest revision names (revision list has no product_id multi-filter;
  // fetch per product for small pages; for larger sets fall back to product_no).
  const rows: MasterDataListItem[] = []
  for (const product of products) {
    let revision: ProductRevisionDto | undefined
    try {
      const revPage = await apiGet<BackendPage<ProductRevisionDto>>(
        "/admin/product-revisions",
        {
          product_id: product.id,
          page: 1,
          page_size: 1,
          sort_by: "revision_no",
          sort_dir: "desc",
        }
      )
      revision = revPage.items[0]
    } catch {
      // leave revision undefined
    }
    rows.push(mapProductRow(product, revision))
  }
  return rows
}

async function listSellableItems(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {
    status,
    sku_no: query.q || undefined,
  })
  if (skus.length === 0) return []

  const products = await fetchAllPages<ProductDto>("/admin/products", {})
  const productById = new Map(products.map((p) => [p.id, p]))

  const rows: MasterDataListItem[] = []
  for (const sku of skus) {
    let revision: SkuRevisionDto | undefined
    try {
      const revPage = await apiGet<BackendPage<SkuRevisionDto>>(
        "/admin/sku-revisions",
        {
          sku_id: sku.id,
          page: 1,
          page_size: 1,
          sort_by: "revision_no",
          sort_dir: "desc",
        }
      )
      revision = revPage.items[0]
    } catch {
      // ignore
    }
    rows.push(mapSkuAsSellable(sku, revision, productById.get(sku.product_id)))
  }
  return rows
}

async function listVoucherCategories(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const profiles = await fetchAllPages<VoucherCategoryProfileDto>(
    "/admin/voucher-category-profiles",
    { status }
  )
  if (profiles.length === 0) return []
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {})
  const skuById = new Map(skus.map((s) => [s.id, s]))
  let rows = profiles.map((p) => mapVoucherRow(p, skuById.get(p.sku_id)))
  if (query.q?.trim()) {
    const q = query.q.trim().toLowerCase()
    rows = rows.filter(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        r.stableNo.toLowerCase().includes(q)
    )
  }
  return rows
}

async function listWarehouses(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const warehouses = await fetchAllPages<WarehouseDto>("/admin/warehouses", {
    status,
    warehouse_code: query.q || undefined,
  })
  const rows: MasterDataListItem[] = []
  for (const wh of warehouses) {
    let revision: WarehouseRevisionDto | undefined
    try {
      const revPage = await apiGet<BackendPage<WarehouseRevisionDto>>(
        "/admin/warehouse-revisions",
        {
          warehouse_id: wh.id,
          page: 1,
          page_size: 1,
          sort_by: "revision_no",
          sort_dir: "desc",
        }
      )
      revision = revPage.items[0]
    } catch {
      // ignore
    }
    rows.push(mapWarehouseRow(wh, revision))
  }
  return rows
}

async function listSuppliers(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  const suppliers = await fetchAllPages<SupplierDto>("/admin/suppliers", {
    status,
    keyword: query.q || undefined,
  })
  if (suppliers.length === 0) return []

  const parties = await fetchAllPages<PartyDto>("/admin/parties", {})
  const partyById = new Map(parties.map((p) => [p.id, p]))

  const rows: MasterDataListItem[] = []
  for (const supplier of suppliers) {
    let partyName: string | undefined
    const party = partyById.get(supplier.party_id)
    if (party?.current_revision_id) {
      try {
        const revPage = await apiGet<BackendPage<PartyRevisionDto>>(
          `/admin/parties/${party.id}/revisions`,
          { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" }
        )
        partyName = revPage.items[0]?.legal_name
      } catch {
        partyName = party.party_no
      }
    } else if (party) {
      partyName = party.party_no
    }

    let profile: CommercialProfileDto | null = null
    try {
      const detail = await apiGet<SupplierDetailDto>(
        `/admin/suppliers/${supplier.id}`
      )
      profile = detail.current_profile
      if (!partyName && detail.party_no) partyName = detail.party_no
    } catch {
      // ignore
    }
    rows.push(mapSupplierRow(supplier, partyName, profile))
  }
  return rows
}

// ---------------------------------------------------------------------------
// Center / detail
// ---------------------------------------------------------------------------

function baseCenter(
  resource: MasterDataResource,
  row: MasterDataListItem,
  extras: Partial<MasterDataCenterView> = {}
): MasterDataCenterView {
  return {
    resource,
    stableId: row.stableId,
    stableNo: row.stableNo,
    name: row.name,
    lifecycleStatus: row.lifecycleStatus,
    lifecycleStatusLabel: row.lifecycleStatusLabel,
    lifecycleTone: row.lifecycleTone,
    scheduledLifecycleStatus: row.scheduledLifecycleStatus,
    scheduledLifecycleLabel: row.scheduledLifecycleLabel,
    revisionTiming: row.revisionTiming,
    revisionTimingLabel: row.revisionTimingLabel,
    lockVersion: row.lockVersion,
    currentRevision: {
      revisionId: row.currentRevisionId,
      revisionNo: row.revisionNo,
      name: row.name,
      effectiveFrom: row.effectiveFrom,
      effectiveTo: row.effectiveTo,
      changeReason: "—",
      actor: "—",
      fields: row.keyFacts.map((f) => ({ label: f.label, value: f.value })),
    },
    revisionTimeline: [
      {
        id: row.currentRevisionId,
        revisionNo: row.revisionNo,
        revisionTiming: row.revisionTiming === "FUTURE" ? "FUTURE" : "CURRENT",
        timingLabel: row.revisionTimingLabel,
        nameSnapshot: row.name,
        actor: "—",
        effectiveFrom: row.effectiveFrom,
        effectiveTo: row.effectiveTo,
        changeReason: "—",
        isCurrent: true,
        lifecycleAtRevision: row.lifecycleStatus,
      },
    ],
    selectorEligibility: row.selectorEligibility,
    usageSummary: {
      historicalReferenceCount: 0,
      note: "引用摘要由后端投影提供；当前接口未返回业务引用数。",
    },
    sensitiveFields: [],
    resourceFacts: [...row.keyFacts],
    allowedActions: row.allowedActions,
    actionBlockers: row.actionBlockers,
    auditEvents: [],
    sections: ["overview", "versions", "relations", "audit"],
    ...extras,
  }
}

async function centerCategory(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const items = await fetchAllPages<ProductCategoryDto>(
    "/admin/product-categories",
    {}
  )
  const dto = items.find((c) => c.id === stableId)
  if (!dto) return null
  const byId = new Map(items.map((c) => [c.id, c]))
  const row = mapCategoryRow(dto)
  const parentName = dto.parent_category_id
    ? (byId.get(dto.parent_category_id)?.name ?? "（未知上级）")
    : "（根分类）"
  const facts = [
    { label: "分类代码", value: dto.category_code },
    { label: "上级分类", value: parentName },
    { label: "适用商品类型", value: productKindLabel(dto.product_kind) },
  ]
  return baseCenter("categories", { ...row, keyFacts: facts }, {
    resourceFacts: facts,
    currentRevision: {
      revisionId: dto.id,
      revisionNo: dto.version,
      name: dto.name,
      effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
      changeReason: "—",
      actor: "—",
      fields: facts,
    },
  })
}

async function centerBrand(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const items = await fetchAllPages<ProductBrandDto>("/admin/product-brands", {})
  const dto = items.find((b) => b.id === stableId)
  if (!dto) return null
  const row = mapBrandRow(dto)
  return baseCenter("brands", row)
}

async function centerProduct(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const products = await fetchAllPages<ProductDto>("/admin/products", {})
  const product = products.find((p) => p.id === stableId)
  if (!product) return null

  const revisions = await fetchAllPages<ProductRevisionDto>(
    "/admin/product-revisions",
    { product_id: stableId, sort_by: "revision_no", sort_dir: "desc" }
  )
  const currentRev = revisions[0]
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {
    product_id: stableId,
  })

  // Units / categories / brands for labels
  const units = await fetchAllPages<UnitOfMeasureDto>("/admin/unit-of-measures", {})
  const unitById = new Map(units.map((u) => [u.id, u]))
  const categories = await fetchAllPages<ProductCategoryDto>(
    "/admin/product-categories",
    {}
  )
  const brands = await fetchAllPages<ProductBrandDto>("/admin/product-brands", {})

  // Best-effort: current revision does not embed category/brand/media/specs.
  // SKU rows carry base unit + signature; full SPU media/spec dims are backend gaps.
  const skuFields: ProductSkuFields[] = []
  for (const sku of skus) {
    let rev: SkuRevisionDto | undefined
    try {
      const page = await apiGet<BackendPage<SkuRevisionDto>>(
        "/admin/sku-revisions",
        {
          sku_id: sku.id,
          page: 1,
          page_size: 1,
          sort_by: "revision_no",
          sort_dir: "desc",
        }
      )
      rev = page.items[0]
    } catch {
      // ignore
    }
    const unit = unitById.get(sku.base_unit_id)
    skuFields.push({
      skuId: sku.id,
      specificationSignature: sku.specification_signature,
      skuNo: sku.sku_no,
      attributeValues: [],
      specLabel: sku.specification_signature || "（无规格）",
      barcode: rev?.barcode ?? undefined,
      mainImage: "",
      salePrice: rev?.sales_visible_price_gross ?? undefined,
      baseUnit: unit?.name ?? unit?.symbol,
      lifecycleStatus: asLifecycle(sku.status),
    })
  }

  const primaryUnit = skus[0]
    ? unitById.get(skus[0].base_unit_id)
    : undefined

  // Category/brand IDs are not on ProductView — leave empty strings (backend gap).
  const productDetail = {
    description: undefined as string | undefined,
    baseUnitId: primaryUnit?.id ?? "",
    baseUnitCode: primaryUnit?.unit_code ?? "",
    baseUnit: primaryUnit?.name ?? primaryUnit?.symbol ?? "",
    categoryId: "",
    category: "",
    brandId: "",
    brand: "",
    carouselImages: [] as string[],
    detailImages: [] as string[],
    specs: [] as { name: string; values: readonly string[] }[],
    skus: skuFields,
  }

  const row = mapProductRow(product, currentRev)
  const timeline: RevisionTimelineEntry[] = revisions.map((r, index) => ({
    id: r.id,
    revisionNo: r.revision_no,
    revisionTiming:
      index === 0
        ? isFutureDate(r.effective_from)
          ? ("FUTURE" as const)
          : ("CURRENT" as const)
        : ("HISTORICAL" as const),
    timingLabel:
      index === 0
        ? isFutureDate(r.effective_from)
          ? "待生效"
          : "当前生效"
        : "已结束",
    nameSnapshot: r.name,
    actor: "—",
    effectiveFrom: r.effective_from,
    changeReason: "—",
    isCurrent: index === 0,
    lifecycleAtRevision: asLifecycle(r.status),
  }))

  // silence unused if catalogs empty (kept for future enrichment)
  void categories
  void brands

  return baseCenter("products", row, {
    productKind: product.product_kind,
    productDetail,
    productConstraints: {
      baseUnit: productDetail.baseUnit,
      hasFormalReferences: false,
      skuCount: skuFields.length,
    },
    revisionTimeline:
      timeline.length > 0
        ? timeline
        : baseCenter("products", row).revisionTimeline,
    currentRevision: {
      revisionId: currentRev?.id ?? product.id,
      revisionNo: currentRev?.revision_no ?? product.version,
      name: currentRev?.name ?? product.product_no,
      effectiveFrom:
        currentRev?.effective_from ?? tsToIso(product.created_at).slice(0, 10),
      changeReason: "—",
      actor: "—",
      fields: row.keyFacts.map((f) => ({ label: f.label, value: f.value })),
    },
  })
}

async function centerSellable(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {})
  const sku = skus.find((s) => s.id === stableId)
  if (!sku) return null
  let revision: SkuRevisionDto | undefined
  try {
    const page = await apiGet<BackendPage<SkuRevisionDto>>(
      "/admin/sku-revisions",
      {
        sku_id: sku.id,
        page: 1,
        page_size: 1,
        sort_by: "revision_no",
        sort_dir: "desc",
      }
    )
    revision = page.items[0]
  } catch {
    // ignore
  }
  const products = await fetchAllPages<ProductDto>("/admin/products", {})
  const product = products.find((p) => p.id === sku.product_id)
  const row = mapSkuAsSellable(sku, revision, product)
  return baseCenter("sellable-items", row)
}

async function centerVoucher(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const profiles = await fetchAllPages<VoucherCategoryProfileDto>(
    "/admin/voucher-category-profiles",
    {}
  )
  const profile = profiles.find((p) => p.id === stableId)
  if (!profile) return null
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {})
  const sku = skus.find((s) => s.id === profile.sku_id)
  const row = mapVoucherRow(profile, sku)
  return baseCenter("voucher-categories", row)
}

async function centerWarehouse(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const warehouses = await fetchAllPages<WarehouseDto>("/admin/warehouses", {})
  const wh = warehouses.find((w) => w.id === stableId)
  if (!wh) return null
  const revisions = await fetchAllPages<WarehouseRevisionDto>(
    "/admin/warehouse-revisions",
    { warehouse_id: stableId, sort_by: "revision_no", sort_dir: "desc" }
  )
  const current = revisions[0]
  const row = mapWarehouseRow(wh, current)
  const timeline: RevisionTimelineEntry[] = revisions.map((r, index) => ({
    id: r.id,
    revisionNo: r.revision_no,
    revisionTiming: index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
    timingLabel: index === 0 ? "当前生效" : "已结束",
    nameSnapshot: r.name,
    actor: "—",
    effectiveFrom: r.effective_from,
    effectiveTo: r.effective_to ?? undefined,
    changeReason: r.change_reason,
    isCurrent: index === 0,
    lifecycleAtRevision: asLifecycle(wh.status),
  }))
  return baseCenter("warehouses", row, {
    warehouseStockSummary: {
      onHandQty: "—",
      reservedQty: "—",
      hasBlockingStock: false,
      w10Href: `/inventory?warehouseId=${encodeURIComponent(wh.id)}`,
      policyNote: "库存摘要由 W10 投影提供；当前接口未返回数量。",
    },
    revisionTimeline:
      timeline.length > 0
        ? timeline
        : baseCenter("warehouses", row).revisionTimeline,
    sensitiveFields: [
      {
        label: "联系人 / 地址",
        maskedValue: "（敏感字段，需授权查看）",
        visibility: "masked",
      },
    ],
  })
}

async function centerSupplier(
  stableId: string
): Promise<MasterDataCenterView | null> {
  let detail: SupplierDetailDto
  try {
    detail = await apiGet<SupplierDetailDto>(`/admin/suppliers/${stableId}`)
  } catch (error) {
    if (isApiError(error) && error.status === 404) return null
    throw error
  }

  let partyName = detail.party_no ?? detail.supplier_no
  try {
    const revPage = await apiGet<BackendPage<PartyRevisionDto>>(
      `/admin/parties/${detail.party_id}/revisions`,
      { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" }
    )
    if (revPage.items[0]?.legal_name) partyName = revPage.items[0].legal_name
  } catch {
    // ignore
  }

  const profile = detail.current_profile
  const row = mapSupplierRow(detail, partyName, profile)

  const [capabilities, qualifications, ratings, profiles] = await Promise.all([
    fetchAllPages<SupplierCapabilityDto>(
      `/admin/suppliers/${stableId}/capabilities`,
      {}
    ).catch(() => [] as SupplierCapabilityDto[]),
    fetchAllPages<SupplierQualificationDto>(
      `/admin/suppliers/${stableId}/qualifications`,
      {}
    ).catch(() => [] as SupplierQualificationDto[]),
    fetchAllPages<SupplierRatingDto>(
      `/admin/suppliers/${stableId}/ratings`,
      {}
    ).catch(() => [] as SupplierRatingDto[]),
    fetchAllPages<CommercialProfileDto>(
      `/admin/suppliers/${stableId}/commercial-profiles`,
      {}
    ).catch(() => [] as CommercialProfileDto[]),
  ])

  const capabilityLabels = capabilities
    .map((c) => c.capability_code)
    .filter(Boolean)
    .join("、")
  const rating = ratings[0]
  const facts = [
    { label: "供应商编号", value: detail.supplier_no },
    { label: "企业主体", value: partyName },
    {
      label: "结算方式",
      value: settlementLabel(profile?.settlement_mode) || "—",
    },
    {
      label: "发票类型",
      value: invoiceLabel(profile?.invoice_type) || "—",
    },
    {
      label: "发票税点",
      value: profile?.invoice_tax_rate ?? "—",
    },
    { label: "能力", value: capabilityLabels || "—" },
    {
      label: "资质",
      value:
        qualifications.length > 0
          ? `${qualifications.length} 项`
          : "—",
    },
    {
      label: "供应商评级",
      value: rating?.rating ?? "—",
    },
  ]

  const timeline: RevisionTimelineEntry[] = profiles.map((p, index) => ({
    id: p.id,
    revisionNo: p.revision_no,
    revisionTiming: index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
    timingLabel: index === 0 ? "当前生效" : "已结束",
    nameSnapshot: partyName,
    actor: "—",
    effectiveFrom: p.valid_from,
    effectiveTo: p.valid_to ?? undefined,
    changeReason: p.change_reason,
    isCurrent: index === 0,
    lifecycleAtRevision: asLifecycle(detail.status),
  }))

  return baseCenter("suppliers", row, {
    resourceFacts: facts,
    currentRevision: {
      revisionId: profile?.id ?? detail.id,
      revisionNo: profile?.revision_no ?? detail.version,
      name: partyName,
      effectiveFrom:
        profile?.valid_from ?? tsToIso(detail.created_at).slice(0, 10),
      effectiveTo: profile?.valid_to ?? undefined,
      changeReason: profile?.change_reason ?? "—",
      actor: "—",
      fields: facts,
    },
    revisionTimeline:
      timeline.length > 0
        ? timeline
        : baseCenter("suppliers", row).revisionTimeline,
    sensitiveFields: [
      {
        label: "银行账号",
        maskedValue: "（请从财务上下文查看）",
        visibility: "masked",
      },
      {
        label: "联系人",
        maskedValue: "（敏感字段，需授权查看）",
        visibility: "masked",
      },
    ],
  })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

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
  fallbackLock?: { version: number; revisionNo: number }
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
  if (error.kind === "Validation" || error.status === 400 || error.status === 422) {
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
  input: CreateMasterDataInput
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
      }
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

function mapProductKindInput(
  kind: string | undefined
): ProductKind {
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
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  const fields = input.fields as BrandFields
  try {
    const created = await apiPost<ProductBrandDto>("/admin/product-brands", {
      brand_code: fields.code,
      name: input.name.trim(),
      status: "active",
    })
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

function mapProductSkus(fields: ProductFields) {
  return fields.skus.map((sku) => ({
    sku_no: sku.skuNo,
    base_unit_id: fields.baseUnitId,
    barcode: sku.barcode || null,
    weight_kg: null,
    volume_m3: null,
    sales_visible_price_gross: sku.salePrice || null,
    market_price: sku.marketPrice || null,
    // Specs: frontend uses free-text dimensions; backend expects attribute codes.
    // Empty entries → no-spec SKU. Full attribute dictionary wiring is a gap.
    spec_entries: [] as { attribute_code: string; attribute_value_code: string }[],
  }))
}

async function createProduct(
  input: CreateMasterDataInput
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

  // product_no is required by backend; form does not collect it.
  // Derive a stable business code from the first SKU no or idempotency key.
  const productNo =
    fields.skus[0]?.skuNo?.replace(/-?\d*$/, "") ||
    `SPU-${input.idempotencyKey.replace(/[^a-zA-Z0-9]/g, "").slice(0, 12)}`

  try {
    const created = await apiPost<ProductDto>("/admin/products", {
      product_no: productNo,
      product_kind: fields.productKind,
      name: input.name.trim(),
      description: fields.description || null,
      specification: null,
      category_id: fields.categoryId,
      brand_id: fields.brandId,
      status: "active",
      effective_from: input.effectiveFrom,
      effective_to: input.effectiveTo || null,
      carousel_media: [],
      detail_media: [],
      skus: mapProductSkus(fields),
    })
    return {
      outcome: "succeeded",
      stableId: created.id,
      stableNo: created.product_no,
      revisionId: created.id,
      revisionNo: 1,
      revisionState: isFutureDate(input.effectiveFrom) ? "FUTURE" : "CURRENT",
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

async function createVoucher(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  const fields = input.fields as VoucherCategoryFields
  // Backend requires an existing VOUCHER SKU id. Frontend form collects sku code only.
  // Attempt to resolve sku_no → id.
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {
    sku_no: fields.sku,
  })
  const sku =
    skus.find((s) => s.sku_no === fields.sku) ??
    skus.find((s) => s.sku_no.includes(fields.sku))
  if (!sku) {
    return {
      outcome: "blocked",
      code: "VOUCHER_SKU_NOT_FOUND",
      message: "找不到对应的卡券 SKU，请先创建 VOUCHER 类型商品与 SKU。",
      detail: `sku=${fields.sku}`,
    }
  }
  try {
    const created = await apiPost<VoucherCategoryProfileDto>(
      "/admin/voucher-category-profiles",
      {
        sku_id: sku.id,
        description: fields.description?.trim() || input.name.trim(),
        status: "active",
      }
    )
    return {
      outcome: "succeeded",
      stableId: created.id,
      stableNo: sku.sku_no,
      revisionId: created.id,
      revisionNo: created.revision_no,
      revisionState: "CURRENT",
      effectiveFrom: input.effectiveFrom,
      recordedAt: isoNow(),
      actor: "—",
      changeReason: input.changeReason || "新建",
      reference: `MD-CREATE-VC-${sku.sku_no}`,
      nextActions: ["查看详情", "更新资料"],
    }
  } catch (error) {
    return mapMutationError(error)
  }
}

async function createSupplier(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  const fields = input.fields as SupplierFields
  // Backend requires existing party_id + settlement fields.
  // Attempt to find/create party by company name is not available as search-by-name;
  // register gap if party cannot be resolved. Prefer matching by party list legal name.
  const parties = await fetchAllPages<PartyDto>("/admin/parties", {})
  let partyId: string | undefined
  for (const party of parties) {
    try {
      const revPage = await apiGet<BackendPage<PartyRevisionDto>>(
        `/admin/parties/${party.id}/revisions`,
        { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" }
      )
      if (revPage.items[0]?.legal_name === fields.company) {
        partyId = party.id
        break
      }
    } catch {
      // continue
    }
  }

  if (!partyId) {
    // Create party first so supplier can reference it.
    try {
      const partyNo = `PTY-${input.idempotencyKey.replace(/[^a-zA-Z0-9]/g, "").slice(0, 10)}`
      const party = await apiPost<PartyDto>("/admin/parties", {
        party_no: partyNo,
        legal_name: fields.company || input.name.trim(),
        short_name: null,
        unified_credit_code: fields.taxNo || null,
        effective_from: input.effectiveFrom,
        effective_to: null,
        change_reason: input.changeReason || "供应商主体",
        status: "active",
      })
      partyId = party.id
    } catch (error) {
      return mapMutationError(error)
    }
  }

  // signing/payment entity: reuse the same party when form leaves them blank.
  const supplierNo = `SUP-${input.idempotencyKey.replace(/[^a-zA-Z0-9]/g, "").slice(0, 10)}`
  try {
    const created = await apiPost<SupplierDto>("/admin/suppliers", {
      party_id: partyId,
      supplier_no: supplierNo,
      default_payment_term_id: null,
      settlement_mode: settlementToBackend(fields.settlement),
      reconciliation_cycle: "monthly",
      payment_term_snapshot: fields.settlement || "默认付款条件",
      invoice_type: invoiceToBackend(fields.invoiceType),
      invoice_tax_rate: fields.invoiceTaxRate || "0.13",
      signing_entity_party_id: partyId,
      payment_entity_party_id: partyId,
      valid_from: input.effectiveFrom,
      valid_to: input.effectiveTo || null,
      change_reason: input.changeReason || "新建",
      status: "active",
    })
    return {
      outcome: "succeeded",
      stableId: created.id,
      stableNo: created.supplier_no,
      revisionId: created.current_commercial_profile_revision_id ?? created.id,
      revisionNo: 1,
      revisionState: "CURRENT",
      effectiveFrom: input.effectiveFrom,
      recordedAt: isoNow(),
      actor: "—",
      changeReason: input.changeReason || "新建",
      reference: `MD-CREATE-${created.supplier_no}`,
      nextActions: ["查看详情", "更新资料"],
    }
  } catch (error) {
    return mapMutationError(error)
  }
}

async function createSellable(
  input: CreateMasterDataInput
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

// ---------------------------------------------------------------------------
// Public API (stable signatures for queries.ts)
// ---------------------------------------------------------------------------

export async function fetchMasterDataList(
  query: MasterDataListQuery
): Promise<MasterDataListResult> {
  let rows: MasterDataListItem[]
  switch (query.resource) {
    case "categories":
      rows = await listCategories(query)
      break
    case "brands":
      rows = await listBrands(query)
      break
    case "products":
      rows = await listProducts(query)
      break
    case "sellable-items":
      rows = await listSellableItems(query)
      break
    case "voucher-categories":
      rows = await listVoucherCategories(query)
      break
    case "warehouses":
      rows = await listWarehouses(query)
      break
    case "suppliers":
      rows = await listSuppliers(query)
      break
    default:
      rows = []
  }

  // Client-side residual filters the server cannot express (revisionTiming / metricKey)
  if (query.revisionTiming && query.revisionTiming !== "all") {
    rows = rows.filter((r) =>
      query.revisionTiming === "future"
        ? r.revisionTiming === "FUTURE"
        : r.revisionTiming === "CURRENT"
    )
  }
  if (query.metricKey && query.metricKey !== "all") {
    const key = query.metricKey
    rows = rows.filter((r) => {
      if (key === "enabled") return r.lifecycleStatus === "ENABLED"
      if (key === "disabled") return r.lifecycleStatus === "DISABLED"
      if (key === "pending") return r.revisionTiming === "FUTURE"
      if (key === "expiring") return r.metricTags.includes("expiring")
      return true
    })
  }

  return wrapListResult(query.resource, rows)
}

export async function fetchMasterDataCenter(
  resource: MasterDataResource,
  stableId: string
): Promise<MasterDataCenterView | null> {
  switch (resource) {
    case "categories":
      return centerCategory(stableId)
    case "brands":
      return centerBrand(stableId)
    case "products":
      return centerProduct(stableId)
    case "sellable-items":
      return centerSellable(stableId)
    case "voucher-categories":
      return centerVoucher(stableId)
    case "warehouses":
      return centerWarehouse(stableId)
    case "suppliers":
      return centerSupplier(stableId)
    default:
      return null
  }
}

export async function createMasterDataObject(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  if (input.resource === "warehouses") return blockedWarehouse()
  switch (input.resource) {
    case "categories":
      return createCategory(input)
    case "brands":
      return createBrand(input)
    case "products":
      return createProduct(input)
    case "voucher-categories":
      return createVoucher(input)
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
  input: CreateRevisionInput
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
          }
        )
        // parent move is a separate endpoint
        if (fields.parentId !== undefined) {
          try {
            await apiPut(
              `/admin/product-categories/${input.stableId}/parent`,
              {
                version: updated.version,
                parent_category_id: fields.parentId || null,
              }
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
        const updated = await apiPut<ProductBrandDto>(
          `/admin/product-brands/${input.stableId}`,
          {
            version: input.expectedLockVersion,
            name: input.name.trim(),
          }
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
      case "products": {
        const fields = input.fields as ProductFields
        if (!fields.categoryId || !fields.brandId || !fields.baseUnitId) {
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
            name: input.name.trim(),
            description: fields.description || null,
            specification: null,
            category_id: fields.categoryId,
            brand_id: fields.brandId,
            status: "active",
            effective_from: input.effectiveFrom,
            effective_to: input.effectiveTo || null,
            carousel_media: [],
            detail_media: [],
            skus: mapProductSkus(fields),
          }
        )
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.product_no,
          revisionId: updated.id,
          revisionNo: updated.version,
          revisionState: isFutureDate(input.effectiveFrom) ? "FUTURE" : "CURRENT",
          effectiveFrom: input.effectiveFrom,
          recordedAt: isoNow(),
          actor: "—",
          changeReason: input.changeReason,
          reference: `MD-REV-${updated.product_no}-v${updated.version}`,
          nextActions: ["查看变更历史", "返回列表"],
        }
      }
      case "suppliers": {
        // Supplier update only allows status / payment term; commercial profile is append.
        const fields = input.fields as SupplierFields
        const updated = await apiPut<SupplierDto>(
          `/admin/suppliers/${input.stableId}`,
          {
            version: input.expectedLockVersion,
            status: undefined,
            default_payment_term_id: null,
          }
        )
        // Append commercial profile when settlement fields present
        if (fields.settlement || fields.invoiceType) {
          try {
            await apiPost(
              `/admin/suppliers/${input.stableId}/commercial-profiles`,
              {
                settlement_mode: settlementToBackend(fields.settlement),
                reconciliation_cycle: "monthly",
                payment_term_snapshot: fields.settlement || "默认付款条件",
                invoice_type: invoiceToBackend(fields.invoiceType),
                invoice_tax_rate: fields.invoiceTaxRate || "0.13",
                signing_entity_party_id: updated.party_id,
                payment_entity_party_id: updated.party_id,
                valid_from: input.effectiveFrom,
                valid_to: input.effectiveTo || null,
                change_reason: input.changeReason,
              }
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
          stableNo: updated.supplier_no,
          revisionId:
            updated.current_commercial_profile_revision_id ?? updated.id,
          revisionNo: updated.version,
          revisionState: "CURRENT",
          effectiveFrom: input.effectiveFrom,
          recordedAt: isoNow(),
          actor: "—",
          changeReason: input.changeReason,
          reference: `MD-REV-${updated.supplier_no}-v${updated.version}`,
          nextActions: ["查看变更历史", "返回列表"],
        }
      }
      case "sellable-items":
        return {
          outcome: "blocked",
          code: "SELLABLE_NOT_WRITABLE",
          message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
        }
      case "voucher-categories":
        return {
          outcome: "blocked",
          code: "VOUCHER_PROFILE_NO_UPDATE",
          message: "卡券类目扩展修订暂无更新接口（仅创建）。",
          detail: "backend: POST /admin/voucher-category-profiles only",
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
  input: DisableMasterDataInput
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
          }
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
          }
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
            name: center.name,
            description: detail?.description || null,
            specification: null,
            category_id: detail?.categoryId || "",
            brand_id: detail?.brandId || "",
            status: "disabled",
            effective_from: input.effectiveFrom,
            effective_to: null,
            carousel_media: [],
            detail_media: [],
            skus: (detail?.skus ?? []).map((sku) => ({
              sku_no: sku.skuNo,
              base_unit_id: detail?.baseUnitId || "",
              barcode: sku.barcode || null,
              weight_kg: null,
              volume_m3: null,
              sales_visible_price_gross: sku.salePrice || null,
              market_price: sku.marketPrice || null,
              spec_entries: [],
            })),
          }
        )
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.product_no,
          revisionId: updated.id,
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
          }
        )
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.supplier_no,
          revisionId:
            updated.current_commercial_profile_revision_id ?? updated.id,
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
          code: "VOUCHER_PROFILE_NO_DISABLE",
          message: "卡券类目扩展修订暂无停用接口。",
        }
      case "sellable-items":
        return {
          outcome: "blocked",
          code: "SELLABLE_NOT_WRITABLE",
          message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
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

/**
 * 幂等查询：后端 master-data 域无统一 idempotency 查询接口。
 * 返回 null 让 UI 走「结果未知」重试路径，不伪造成功。
 */
export async function queryMasterDataIdempotency(
  idempotencyKey: string
): Promise<MasterDataMutationResult | null> {
  void idempotencyKey
  return null
}

/**
 * 敏感字段揭示：party/warehouse 敏感明文仅写入指纹，无通用 reveal 接口。
 * 失败以 ApiError 形态抛出，供 mutation 进入 error 态。
 */
export async function revealMasterDataSensitive(
  revealToken: string
): Promise<string> {
  void revealToken
  const error: ApiError = {
    kind: "Validation",
    message: "无权查看或权限已失效；敏感字段揭示接口尚未提供",
    status: 403,
  }
  throw error
}

// Re-export pure display helpers used by pages (stable import path via queries)
export { buildMasterDataExportCsv, downloadCsv } from "./export-csv"

