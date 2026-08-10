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
  ProductFilterOptions,
  ProductKind,
  ProductListSkuSummary,
  ProductListingStatus,
  ProductSkuFields,
  RevisionTimelineEntry,
  SellableItemFields,
  SupplierFields,
  UnitOfMeasureFields,
  VoucherCategoryFields,
} from "@/features/master-data/types"
import { parseMediaList } from "@/features/master-data/resource-fields"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"

// ---------------------------------------------------------------------------
// Backend DTO shapes (subset used by this feature)
// ---------------------------------------------------------------------------

type EnableStatus = "active" | "disabled"

type BackendPage<T> = Page<T>

type BackendFileAsset = {
  id: string
  storage_object_key: string
  public_url?: string | null
  file_name: string
  content_type: string
  byte_size: number
  security_scan_status: string
  created_by: string
  created_at: number
  version?: number
}

/** 按文件资产 ID 查询详情（含公开访问地址，供媒体回显）。 */
async function fetchFileAsset(assetId: string): Promise<BackendFileAsset | null> {
  try {
    return await apiGet<BackendFileAsset>(`/admin/file-assets/${encodeURIComponent(assetId)}`)
  } catch {
    return null
  }
}

/** 批量解析媒体文件资产为 `assetId → 资产详情`（去重，单条失败降级为缺失）。 */
async function resolveMediaAssets(
  assetIds: readonly string[],
): Promise<Map<string, BackendFileAsset>> {
  const unique = [...new Set(assetIds.filter((id) => id.trim()))]
  const resolved = new Map<string, BackendFileAsset>()
  for (const assetId of unique) {
    const asset = await fetchFileAsset(assetId)
    if (asset) resolved.set(assetId, asset)
  }
  return resolved
}

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
  logo_asset_id?: string | null
  status: EnableStatus
  created_at: number
  version: number
}

type ProductDto = {
  id: string
  product_no: string
  product_kind: ProductKind
  name?: string | null
  category_id?: string | null
  brand_id?: string | null
  status: EnableStatus
  listing_status: "listed" | "partially_listed" | "unlisted"
  listed_sku_count: number
  sku_count: number
  supplied_sku_count?: number
  priced_sku_count?: number
  current_revision_id: string | null
  created_at: number
  version: number
}

type ProductListingDto = {
  product_id: string
  listing_status: "listed" | "partially_listed" | "unlisted"
  listed_sku_count: number
  sku_count: number
}

type ProductRevisionDto = {
  id: string
  product_id: string
  revision_no: number
  name: string
  description: string | null
  specification: string | null
  category_id: string
  brand_id: string
  status: EnableStatus
  effective_from: string
  effective_to: string | null
  media?: Array<{
    id: string
    file_asset_id: string
    media_role: string
    sort_order: number
    alt_text?: string | null
  }>
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
  listing_status: "listed" | "unlisted"
  current_revision_id: string | null
  created_at: number
  version: number
}

type SkuRevisionDto = {
  id: string
  sku_id: string
  revision_no: number
  name: string
  description: string | null
  specification: string | null
  barcode: string | null
  source_main_image_asset_id?: string | null
  status: EnableStatus
  sales_visible_price_gross: string | null
  market_price: string | null
  weight_kg: string | null
  volume_m3: string | null
  effective_from: string
  effective_to: string | null
  created_at: number
  version: number
}

type SellableSkuDto = {
  sku_id: string
  sku_version: number
  sku_revision_id: string
  sku_revision_no: number
  sku_no: string
  product_id: string
  product_no: string
  product_kind: ProductKind
  name: string
  specification_attributes: Array<{
    name: string
    value: string
  }>
  specification: string | null
  barcode: string | null
  base_unit_id: string
  base_unit_code: string | null
  base_unit_name: string | null
  sales_visible_price_gross: string
  market_price: string | null
  main_image_asset_id: string | null
  effective_from: string
  effective_to: string | null
  supplier_count: number
  supply_regions: string[]
  eligibility_as_of: string
}

type SupplierOfferingSummaryDto = {
  sku_id: string
  supplier_id: string
  status: string
  current_revision_id: string | null
}

type VoucherCategoryProfileDto = {
  id: string
  sku_id: string
  sku_no?: string | null
  product_id?: string | null
  product_version?: number | null
  name?: string | null
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
  party_no: string | null
  legal_name: string | null
  short_name: string | null
  party_version: number | null
  supplier_no: string
  default_payment_term_id: string | null
  current_commercial_profile_revision_id: string | null
  status: EnableStatus
  version: number
  created_at: number
  current_profile: CommercialProfileDto | null
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
  signing_entity_name: string | null
  payment_entity_party_id: string | null
  payment_entity_name: string | null
  change_reason: string
  version: number
  created_at: number
}

type SupplierDetailDto = SupplierDto & {
  party_status: string
  unified_credit_code: string | null
  contacts: PartyContactDto[]
  addresses: PartyAddressDto[]
  tax_profiles: PartyTaxProfileDto[]
  bank_accounts: PartyBankAccountDto[]
  capabilities: SupplierCapabilityDto[]
  qualifications: SupplierQualificationDto[]
  ratings: SupplierRatingDto[]
  commercial_profiles: CommercialProfileDto[]
  sensitive_fields: SupplierSensitiveFieldDto[]
}

type SupplierSensitiveFieldDto = {
  label: string
  masked_value: string
  reveal_token: string
  expires_at: number
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
  capability_ids: string[]
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

type SupplierProfileMutationDto = {
  supplier_id: string
  supplier_no: string
  revision_id: string
  revision_no: number
  supplier_version: number
  effective_from: string
  recorded_at: number
  change_reason: string
}

/** 主体联系人（列表不含 mobile 明文，可用 telephone 回显）。 */
type PartyContactDto = {
  id: string
  party_id: string
  contact_name: string
  title: string | null
  telephone: string | null
  mobile_masked: string
  email: string | null
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

/** 银行账户（列表不含账号明文）。 */
type PartyBankAccountDto = {
  id: string
  bank_account_no: string
  party_id: string
  account_name: string
  bank_name: string
  account_number_masked: string
  bank_branch_name: string | null
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

type PartyAddressDto = {
  id: string
  party_id: string
  address_type: string
  contact_name: string | null
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

type PartyTaxProfileDto = {
  id: string
  party_id: string
  tax_no: string
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
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

/**
 * 生成业务编号（前端未暴露编号录入时的临时唯一码）。
 *
 * 格式：`{prefix}-{timestamp36}{random36}`，避免把幂等键前缀截断后
 * 拼成固定编号（例如 `create-supplier-...` → 永远是 `PTY-createsupp`）。
 */
function genBusinessCode(prefix: string): string {
  const stamp = Date.now().toString(36).toUpperCase()
  const rand = Math.random().toString(36).slice(2, 8).toUpperCase()
  return `${prefix}-${stamp}${rand}`
}

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

/** 后端 capability_code → 表单多选中文标签。 */
const capabilityLabel = (code: string | undefined): string => {
  switch (code) {
    case "physical":
      return "实物商品"
    case "virtual":
      return "虚拟商品"
    case "offline_service":
      return "线下服务"
    case "api":
      return "API"
    case "printing":
      return "印刷"
    default:
      return code ?? ""
  }
}

const capabilityToBackend = (label: string): string | null => {
  switch (label.trim()) {
    case "实物商品":
      return "physical"
    case "虚拟商品":
      return "virtual"
    case "线下服务":
      return "offline_service"
    case "API":
      return "api"
    case "印刷":
      return "printing"
    default:
      return null
  }
}

/** 后端评级代码（A/B/C/D 或已是「A 级」）→ 表单选项。 */
const ratingLabel = (rating: string | undefined): string => {
  if (!rating) return ""
  const trimmed = rating.trim()
  if (/^[ABCD]$/i.test(trimmed)) return `${trimmed.toUpperCase()} 级`
  if (/^[ABCD]\s*级$/i.test(trimmed)) {
    return `${trimmed.charAt(0).toUpperCase()} 级`
  }
  return trimmed
}

const ratingToBackend = (label: string | undefined): string => {
  if (!label) return "C"
  const m = label.trim().match(/^([ABCD])/i)
  return m ? m[1].toUpperCase() : "C"
}

/**
 * 经营类目暂无独立后端字段；编码进商务版本 `payment_term_snapshot`
 *（结算方式本身走 `settlement_mode` 枚举，快照仅作展示/回填载体）。
 * 标记串需稳定，加载时原样解析。
 */
const BUSINESS_CATEGORY_MARK = "｜经营类目："

/** 结算文案 + 经营类目 → 付款条件快照（≤64 字）。 */
const buildPaymentTermSnapshot = (
  settlement: string | undefined,
  businessCategory: string | undefined
): string => {
  const base = (settlement?.trim() || "默认付款条件").slice(0, 64)
  const cat = businessCategory?.trim()
  if (!cat) return base
  const encoded = `${base}${BUSINESS_CATEGORY_MARK}${cat}`
  return [...encoded].slice(0, 64).join("")
}

/** 从付款条件快照解析经营类目（无标记则空）。 */
const parseBusinessCategoryFromSnapshot = (
  snapshot: string | null | undefined
): string => {
  if (!snapshot) return ""
  const idx = snapshot.indexOf(BUSINESS_CATEGORY_MARK)
  if (idx < 0) return ""
  return snapshot.slice(idx + BUSINESS_CATEGORY_MARK.length).trim()
}

/** 百分制评分：合法则返回 0–100 整数，否则 undefined。 */
const parseScore100 = (raw: string | undefined): number | undefined => {
  if (raw == null || !String(raw).trim()) return undefined
  const n = Number.parseInt(String(raw).trim(), 10)
  if (!Number.isFinite(n) || n < 0 || n > 100) return undefined
  return n
}

/** 将用户输入的整数百分数转换为后端 [0, 1) 税率字符串。 */
const normalizeTaxRate = (raw: string | undefined): string => {
  const text = (raw ?? "").trim().replace(/%$/, "")
  if (!text) return "0.13"
  if (!/^(0|[1-9]\d?)$/.test(text)) return "0.13"
  const value = Number(text)
  return String(value / 100)
}

/** 将后端 [0, 1) 税率转换为页面百分数输入值。 */
const taxRatePercent = (raw: string | null | undefined): string => {
  if (!raw?.trim()) return ""
  const value = Number(raw)
  if (!Number.isFinite(value) || value < 0 || value >= 1) return ""
  return String(Math.round(value * 100))
}

const pickDefaultOrFirst = <T extends { is_default?: boolean }>(
  items: readonly T[]
): T | undefined => items.find((item) => item.is_default) ?? items[0]

/** 事实行：空值不写入，避免编辑回填被「—」占位污染。 */
function fact(
  label: string,
  value: string | number | null | undefined
): { label: string; value: string } | null {
  if (value === null || value === undefined) return null
  const text = String(value).trim()
  if (!text || text === "—") return null
  return { label, value: text }
}

function factsOf(
  ...rows: Array<{ label: string; value: string } | null>
): Array<{ label: string; value: string }> {
  return rows.filter(
    (row): row is { label: string; value: string } => row !== null
  )
}

const commonActions = (
  resource: MasterDataResource,
  lifecycle: LifecycleStatus
): Pick<MasterDataListItem, "allowedActions" | "actionBlockers"> => {
  if (resource === "sellable-items") {
    return {
      allowedActions: ["VIEW", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "CREATE_REVISION",
          code: "SELLABLE_READ_ONLY",
          message: "公司商品池由销售资格实时计算，请在公司商品中维护销售资料。",
        },
        {
          action: "DISABLE",
          code: "SELLABLE_READ_ONLY",
          message: "公司商品池没有独立启停状态，请维护公司 SKU 或供应商供给。",
        },
      ],
    }
  }
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
  // 卡券类目：仅新建 + 编辑；不提供查看详情 / 停用。
  if (resource === "voucher-categories") {
    return {
      allowedActions: ["CREATE_REVISION", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "VIEW",
          code: "VOUCHER_NO_DETAIL",
          message: "卡券类目在列表原地编辑，不提供独立查看。",
        },
        {
          action: "DISABLE",
          code: "VOUCHER_NO_DISABLE",
          message: "卡券类目不支持停用。",
        },
      ],
    }
  }
  // 计量单位：列表 Dialog 更新 / 停用，无侧边预览与独立详情。
  if (resource === "unit-of-measures") {
    const allowed: string[] = ["CREATE_REVISION", "EXPORT_ROW"]
    const blockers: Array<{ action: string; code: string; message: string }> = [
      {
        action: "VIEW",
        code: "UNIT_NO_SIDE_PREVIEW",
        message: "计量单位在列表 Dialog 维护，不提供侧边预览。",
      },
    ]
    if (lifecycle === "ENABLED") {
      allowed.push("DISABLE")
    } else {
      blockers.push({
        action: "DISABLE",
        code: "ALREADY_DISABLED",
        message: "资料已停用；不是删除，历史记录仍可查看。",
      })
    }
    return { allowedActions: allowed, actionBlockers: blockers }
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

/** 将后端规范化规格签名还原为 W14 可编辑的 SPU 局部规格名/值。 */
function parseSpecificationSignature(
  signature: string
): Array<{ attributeCode: string; valueCode: string }> {
  if (!signature) return []
  return signature.split("|").flatMap((entry) => {
    const separator = entry.indexOf("=")
    if (separator <= 0) return []
    const attributeCode = entry.slice(0, separator).trim()
    const valueCode = entry.slice(separator + 1).trim()
    return attributeCode && valueCode ? [{ attributeCode, valueCode }] : []
  })
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

function mapUnitOfMeasureRow(dto: UnitOfMeasureDto): MasterDataListItem {
  const lifecycle = asLifecycle(dto.status)
  return {
    objectType: "unit-of-measures",
    stableId: dto.id,
    stableNo: dto.unit_code,
    name: dto.name,
    dictionaryCode: dto.unit_code,
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
      { label: "单位代码", value: dto.unit_code },
      { label: "单位符号", value: dto.symbol },
      { label: "数量小数位", value: String(dto.quantity_scale) },
    ],
    selectorEligibility: [],
    ...commonActions("unit-of-measures", lifecycle),
    lockVersion: dto.version,
    metricTags: [lifecycle === "ENABLED" ? "enabled" : "disabled"],
  }
}

function mapProductRow(
  dto: ProductDto,
  revision?: ProductRevisionDto
): MasterDataListItem {
  const lifecycle = asLifecycle(dto.status)
  const listingStatus = dto.listing_status.toUpperCase() as ProductListingStatus
  const future = revision ? isFutureDate(revision.effective_from) : false
  const productName = revision?.name ?? dto.name ?? dto.product_no
  return {
    objectType: "products",
    stableId: dto.id,
    stableNo: dto.product_no,
    name: productName,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    listingStatus,
    listedSkuCount: dto.listed_sku_count,
    skuCount: dto.sku_count,
    revisionTiming: future ? "FUTURE" : "CURRENT",
    revisionTimingLabel: future ? "待生效" : "当前生效",
    currentRevisionId: revision?.id ?? dto.id,
    displayedRevisionId: revision?.id ?? dto.id,
    revisionNo: revision?.revision_no ?? dto.version,
    effectiveFrom: revision?.effective_from ?? tsToIso(dto.created_at).slice(0, 10),
    keyFacts: [
      { label: "商品编号", value: dto.product_no },
      { label: "商品类型", value: productKindLabel(dto.product_kind) },
      {
        label: "上架 SKU",
        value: `${dto.listed_sku_count}/${dto.sku_count}`,
      },
      {
        label: "有供给 SKU",
        value: `${dto.supplied_sku_count ?? 0}/${dto.sku_count}`,
      },
      {
        label: "已填销售价 SKU",
        value: `${dto.priced_sku_count ?? 0}/${dto.sku_count}`,
      },
    ],
    primaryBlocker: lifecycle === "DISABLED" ? "已停用：历史引用保留" : undefined,
    selectorEligibility: [],
    ...commonActions("products", lifecycle),
    lockVersion: dto.version,
    metricTags: [
      lifecycle === "ENABLED" ? "enabled" : "disabled",
      ...(future ? (["pending"] as const) : []),
    ],
    // 稳定码（PHYSICAL/VOUCHER…），展示文案仍在 keyFacts「商品类型」
    productKind: dto.product_kind,
  }
}

function mapSkuAsSellable(dto: SellableSkuDto): MasterDataListItem {
  const lifecycle = "ENABLED" as const
  const kindLabel = productKindLabel(dto.product_kind)
  const specificationAttributes = dto.specification_attributes ?? []
  const specificationLabel =
    specificationAttributes.length > 0
      ? specificationAttributes
          .map((attribute) => `${attribute.name}：${attribute.value}`)
          .join(" / ")
      : "无规格"
  const baseUnit = dto.base_unit_name ?? dto.base_unit_code ?? "—"
  return {
    objectType: "sellable-items",
    stableId: dto.sku_id,
    stableNo: dto.sku_no,
    name: dto.name,
    lifecycleStatus: lifecycle,
    lifecycleStatusLabel: lifecycleLabel(lifecycle),
    lifecycleTone: lifecycleTone(lifecycle),
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: dto.sku_revision_id,
    displayedRevisionId: dto.sku_revision_id,
    revisionNo: dto.sku_revision_no,
    effectiveFrom: dto.effective_from,
    effectiveTo: dto.effective_to ?? undefined,
    keyFacts: [
      { label: "SKU", value: dto.sku_no },
      {
        label: "销售价",
        value: `¥${dto.sales_visible_price_gross}`,
      },
      {
        label: "商品编号",
        value: dto.product_no,
      },
      ...(dto.base_unit_name || dto.base_unit_code
        ? [{ label: "基础单位", value: dto.base_unit_name ?? dto.base_unit_code! }]
        : []),
      ...(kindLabel
        ? [{ label: "商品类型", value: kindLabel }]
        : []),
      { label: "有效供应商", value: `${dto.supplier_count} 家` },
      ...(dto.supply_regions.length > 0
        ? [{ label: "可供区域", value: dto.supply_regions.join("、") }]
        : []),
    ],
    selectorEligibility: [],
    ...commonActions("sellable-items", lifecycle),
    lockVersion: dto.sku_version,
    metricTags: ["enabled"],
    productKind: dto.product_kind,
    sellableItem: {
      productId: dto.product_id,
      productNo: dto.product_no,
      specificationAttributes,
      specificationLabel,
      barcode: dto.barcode ?? undefined,
      baseUnit,
      productKindLabel: kindLabel,
      salesVisiblePriceGross: dto.sales_visible_price_gross,
      marketPrice: dto.market_price ?? undefined,
      supplierCount: dto.supplier_count,
      supplyRegions: dto.supply_regions,
      eligibilityAsOf: dto.eligibility_as_of,
    },
  }
}

function mapVoucherRow(
  profile: VoucherCategoryProfileDto,
  sku?: SkuDto
): MasterDataListItem {
  const lifecycle = asLifecycle(profile.status)
  const skuNo =
    profile.sku_no ?? sku?.sku_no ?? profile.sku_id
  const displayName = profile.name?.trim() || profile.description
  // 稳定身份 = SKU（创建后不变）；列表按 SKU 聚合最新扩展修订。
  return {
    objectType: "voucher-categories",
    stableId: profile.sku_id,
    stableNo: skuNo,
    name: displayName,
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
      { label: "卡券 SKU", value: skuNo },
      { label: "说明", value: profile.description },
    ],
    primaryBlocker: lifecycle === "DISABLED" ? "已停用" : undefined,
    selectorEligibility: [],
    ...commonActions("voucher-categories", lifecycle),
    lockVersion: profile.product_version ?? profile.version,
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
  partyName = supplier.legal_name ?? supplier.short_name ?? undefined,
  profile: CommercialProfileDto | null = supplier.current_profile
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
    effectiveFrom: tsToIso(
      profile?.created_at ?? supplier.created_at
    ).slice(0, 10),
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

async function listUnitOfMeasures(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  const status =
    query.lifecycleStatus === "enabled"
      ? "active"
      : query.lifecycleStatus === "disabled"
        ? "disabled"
        : undefined
  // 仅按 status 拉全量（字典体量小）；代码/名称/符号在本地模糊匹配
  const items = await fetchAllPages<UnitOfMeasureDto>(
    "/admin/unit-of-measures",
    { status }
  )
  const rows = items.map(mapUnitOfMeasureRow)
  const q = query.q?.trim().toLowerCase()
  if (!q) return rows
  return rows.filter((row) => {
    const hay = [row.stableNo, row.name, ...row.keyFacts.map((f) => f.value)]
      .join(" ")
      .toLowerCase()
    return hay.includes(q)
  })
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
    keyword: query.q || undefined,
    product_kind: query.productKind,
    category_id: query.productCategoryId,
    brand_id: query.productBrandId,
    supplier_id: query.productSupplierId,
    listing_status: query.productListingStatus,
    supply_coverage: query.productSupplyCoverage,
    sales_price_min: query.productSalesPriceMin,
    sales_price_max: query.productSalesPriceMax,
  })
  return products.map((product) => mapProductRow(product))
}

/** 读取商品筛选使用的启用分类、品牌与供应商选项。 */
export async function fetchProductFilterOptions(): Promise<ProductFilterOptions> {
  const [categories, brands, suppliers] = await Promise.all([
    fetchAllPages<ProductCategoryDto>("/admin/product-categories", {
      status: "active",
      sort_by: "name",
      sort_dir: "asc",
    }),
    fetchAllPages<ProductBrandDto>("/admin/product-brands", {
      status: "active",
      sort_by: "name",
      sort_dir: "asc",
    }),
    fetchAllPages<SupplierDto>("/admin/suppliers", {
      status: "active",
    }),
  ])
  const supplierOptions = suppliers
    .map((supplier) => ({
      value: supplier.id,
      label: supplier.short_name ?? supplier.legal_name ?? supplier.supplier_no,
      keywords: [
        supplier.supplier_no,
        supplier.party_no,
        supplier.short_name,
        supplier.legal_name,
      ]
        .filter(Boolean)
        .join(" "),
    }))
    .sort((left, right) => left.label.localeCompare(right.label, "zh-CN"))
  return {
    categories: categories.map((category) => ({
      value: category.id,
      label: category.name,
      keywords: `${category.category_code} ${category.name}`,
    })),
    brands: brands.map((brand) => ({
      value: brand.id,
      label: brand.name,
      keywords: `${brand.brand_code} ${brand.name}`,
    })),
    suppliers: supplierOptions,
  }
}

/**
 * 读取商品列表当前页的启用 SKU 与当前销售价。
 *
 * 商品列表接口只返回 SKU 数量；这里按稳定商品 ID 补齐 SKU 当前修订，供列表展示
 * 销售价范围，并为新增供给 Dialog 提供固定 SKU 身份。
 */
export async function fetchProductListSkus(
  productIds: readonly string[],
): Promise<readonly ProductListSkuSummary[]> {
  const selectedProductIds = new Set(productIds.filter(Boolean))
  if (selectedProductIds.size === 0) return []

  const [skus, units] = await Promise.all([
    fetchAllPages<SkuDto>("/admin/skus", {}),
    fetchAllPages<UnitOfMeasureDto>("/admin/unit-of-measures", {}),
  ])
  const unitById = new Map(units.map((unit) => [unit.id, unit]))
  const selectedSkus = skus.filter(
    (sku) =>
      selectedProductIds.has(sku.product_id) &&
      asLifecycle(sku.status) === "ENABLED",
  )

  return Promise.all(
    selectedSkus.map(async (sku) => {
      const revisions = await fetchAllPages<SkuRevisionDto>(
        "/admin/sku-revisions",
        {
          sku_id: sku.id,
          sort_by: "revision_no",
          sort_dir: "desc",
        },
      )
      const revision = sku.current_revision_id
        ? revisions.find((item) => item.id === sku.current_revision_id)
        : undefined
      const unit = unitById.get(sku.base_unit_id)
      return {
        productId: sku.product_id,
        skuId: sku.id,
        skuNo: sku.sku_no,
        skuName: revision?.name ?? sku.sku_no,
        specification:
          revision?.specification ?? sku.specification_signature ?? "默认规格",
        baseUnit: unit?.name ?? unit?.symbol ?? unit?.unit_code ?? "—",
        salesVisiblePriceGross:
          revision?.sales_visible_price_gross ?? undefined,
      }
    }),
  )
}

async function listSellableItems(
  query: MasterDataListQuery
): Promise<MasterDataListItem[]> {
  if (query.lifecycleStatus === "disabled") return []
  const rows = await fetchAllPages<SellableSkuDto>("/admin/sellable-skus", {
    q: query.q || undefined,
  })
  return rows.map(mapSkuAsSellable)
}

/** 整组切换 SPU 下全部当前启用 SKU 的上架状态。 */
export async function updateProductListingStatus(
  productId: string,
  listingStatus: Exclude<ProductListingStatus, "PARTIALLY_LISTED">,
): Promise<ProductListingDto> {
  return apiPut<ProductListingDto>(
    `/admin/products/${encodeURIComponent(productId)}/listing-status`,
    {
      listing_status: listingStatus === "LISTED" ? "listed" : "unlisted",
    },
  )
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
  let profiles = await fetchAllPages<VoucherCategoryProfileDto>(
    "/admin/voucher-category-profiles",
    { status }
  ).catch(() => [] as VoucherCategoryProfileDto[])
  // 状态筛选空结果时回退全量，再按启停客户端过滤
  if (profiles.length === 0 && status) {
    profiles = await fetchAllPages<VoucherCategoryProfileDto>(
      "/admin/voucher-category-profiles",
      {}
    ).catch(() => [] as VoucherCategoryProfileDto[])
    if (query.lifecycleStatus === "enabled") {
      profiles = profiles.filter((p) => asLifecycle(p.status) === "ENABLED")
    } else if (query.lifecycleStatus === "disabled") {
      profiles = profiles.filter((p) => asLifecycle(p.status) === "DISABLED")
    }
  }
  if (profiles.length === 0) return []
  // 每个 SKU 只保留最新扩展修订，避免更新后列表出现多行。
  const latestBySku = new Map<string, VoucherCategoryProfileDto>()
  for (const profile of profiles) {
    const prev = latestBySku.get(profile.sku_id)
    if (!prev || profile.revision_no > prev.revision_no) {
      latestBySku.set(profile.sku_id, profile)
    }
  }
  const skus = await fetchAllPages<SkuDto>("/admin/skus", {}).catch(
    () => [] as SkuDto[]
  )
  const skuById = new Map(skus.map((s) => [s.id, s]))
  let rows = Array.from(latestBySku.values()).map((p) =>
    mapVoucherRow(p, skuById.get(p.sku_id))
  )
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
    capability_codes: joinFilterCodes(query.supplierCapabilityCodes),
    qualification_types: joinFilterCodes(query.supplierQualificationTypes),
    qualification_health: query.supplierQualificationHealth,
  })
  return suppliers.map((supplier) => mapSupplierRow(supplier))
}

/** 规范化多选条件，供后端以逗号分隔的稳定查询参数接收。 */
function joinFilterCodes(values: readonly string[] | undefined): string | undefined {
  if (!values?.length) return undefined
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))]
    .sort()
    .join(",")
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
  const logoAssetId = dto.logo_asset_id?.trim()
  const logoAsset = logoAssetId ? await fetchFileAsset(logoAssetId) : null
  const logoUrl = logoAsset?.public_url?.trim()
  return baseCenter("brands", row, {
    resourceFacts: [
      { label: "品牌代码", value: dto.brand_code },
      {
        label: "品牌 Logo",
        value: logoUrl && logoAsset ? logoAsset.file_name : "—",
      },
    ],
    mediaAssets: logoUrl && logoAsset
      ? {
          logo: [
            {
              fileName: logoAsset.file_name,
              assetId: logoAssetId!,
              url: logoUrl,
            },
          ],
        }
      : undefined,
  })
}

async function centerUnitOfMeasure(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const items = await fetchAllPages<UnitOfMeasureDto>(
    "/admin/unit-of-measures",
    {}
  )
  const dto = items.find((u) => u.id === stableId)
  if (!dto) return null
  const row = mapUnitOfMeasureRow(dto)
  return baseCenter("unit-of-measures", row)
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
  const currentRev = product.current_revision_id
    ? revisions.find((revision) => revision.id === product.current_revision_id)
    : undefined
  if (!currentRev) {
    throw new Error("商品当前修订不存在或已漂移，禁止以历史修订回填编辑表单")
  }
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

  // SPU 媒体与 SKU 主图按 file_asset 引用解析为可访问地址。
  const carouselMedia = (currentRev?.media ?? [])
    .filter((m) => m.media_role === "carousel")
    .sort((a, b) => a.sort_order - b.sort_order)
  const detailMedia = (currentRev?.media ?? [])
    .filter((m) => m.media_role === "detail")
    .sort((a, b) => a.sort_order - b.sort_order)
  const resolvedAssets = await resolveMediaAssets([
    ...carouselMedia.map((m) => m.file_asset_id),
    ...detailMedia.map((m) => m.file_asset_id),
  ])
  const carouselPreviewUrls: Record<string, string> = {}
  const carouselFileAssetIds: Record<string, string> = {}
  const detailPreviewUrls: Record<string, string> = {}
  const detailFileAssetIds: Record<string, string> = {}
  const carouselImages: string[] = []
  const detailImages: string[] = []
  for (const media of carouselMedia) {
    const asset = resolvedAssets.get(media.file_asset_id)
    const name = asset?.file_name ?? media.file_asset_id
    carouselImages.push(name)
    if (asset?.public_url) carouselPreviewUrls[name] = asset.public_url
    carouselFileAssetIds[name] = media.file_asset_id
  }
  for (const media of detailMedia) {
    const asset = resolvedAssets.get(media.file_asset_id)
    const name = asset?.file_name ?? media.file_asset_id
    detailImages.push(name)
    if (asset?.public_url) detailPreviewUrls[name] = asset.public_url
    detailFileAssetIds[name] = media.file_asset_id
  }

  const skuFields: ProductSkuFields[] = []
  const parsedSpecsBySku = new Map(
    skus.map((sku) => [sku.id, parseSpecificationSignature(sku.specification_signature)])
  )
  const specNames = [
    ...new Set(
      [...parsedSpecsBySku.values()].flatMap((entries) =>
        entries.map((entry) => entry.attributeCode)
      )
    ),
  ].sort((left, right) => left.localeCompare(right))
  const specs = specNames.map((name) => ({
    name,
    values: [
      ...new Set(
        [...parsedSpecsBySku.values()].flatMap((entries) =>
          entries
            .filter((entry) => entry.attributeCode === name)
            .map((entry) => entry.valueCode)
        )
      ),
    ],
  }))
  for (const sku of skus) {
    const skuRevisions = await fetchAllPages<SkuRevisionDto>(
      "/admin/sku-revisions",
      { sku_id: sku.id, sort_by: "revision_no", sort_dir: "desc" }
    ).catch(() => [] as SkuRevisionDto[])
    const rev = sku.current_revision_id
      ? skuRevisions.find((revision) => revision.id === sku.current_revision_id)
      : undefined
    if (!rev) {
      throw new Error(`SKU ${sku.sku_no} 的当前修订不存在或已漂移，禁止编辑`)
    }
    const unit = unitById.get(sku.base_unit_id)
    const parsedSpecs = parsedSpecsBySku.get(sku.id) ?? []
    const valuesByAttribute = new Map(
      parsedSpecs.map((entry) => [entry.attributeCode, entry.valueCode])
    )
    const attributeValues = specNames.map(
      (name) => valuesByAttribute.get(name) ?? ""
    )
    const mainImageAssetId = rev?.source_main_image_asset_id?.trim()
    const mainAsset = mainImageAssetId
      ? await fetchFileAsset(mainImageAssetId)
      : null
    skuFields.push({
      skuId: sku.id,
      skuRevisionId: rev?.id,
      requiresExplicitReenable: asLifecycle(sku.status) === "DISABLED",
      specificationSignature: sku.specification_signature,
      skuNo: sku.sku_no,
      attributeValues,
      specLabel:
        (rev?.specification ??
          parsedSpecs
            .map((entry) => `${entry.attributeCode}：${entry.valueCode}`)
            .join(" / ")) || "默认规格",
      barcode: rev?.barcode ?? undefined,
      mainImage: mainAsset?.file_name ?? "",
      mainImagePreviewUrl: mainAsset?.public_url ?? undefined,
      mainImageAssetId: mainAsset?.id ?? undefined,
      salePrice: rev?.sales_visible_price_gross ?? undefined,
      marketPrice: rev?.market_price ?? undefined,
      baseUnit: unit?.name ?? unit?.symbol,
      listingStatus:
        sku.listing_status === "listed" ? "LISTED" : "UNLISTED",
      lifecycleStatus: asLifecycle(sku.status),
    })
  }

  const primaryUnit = skus[0]
    ? unitById.get(skus[0].base_unit_id)
    : undefined

  const category = categories.find((item) => item.id === currentRev?.category_id)
  const brand = brands.find((item) => item.id === currentRev?.brand_id)
  const productDetail = {
    lifecycleStatus: asLifecycle(product.status),
    productNo: product.product_no,
    description: currentRev?.description ?? undefined,
    specification: currentRev?.specification ?? undefined,
    baseUnitId: primaryUnit?.id ?? "",
    baseUnitCode: primaryUnit?.unit_code ?? "",
    baseUnit: primaryUnit?.name ?? primaryUnit?.symbol ?? "",
    categoryId: currentRev?.category_id ?? "",
    category: category?.name ?? "",
    brandId: currentRev?.brand_id ?? "",
    brand: brand?.name ?? "",
    carouselImages,
    detailImages,
    carouselPreviewUrls,
    detailPreviewUrls,
    carouselFileAssetIds,
    detailFileAssetIds,
    specs,
    skus: skuFields,
  }

  const row = mapProductRow(product, currentRev)
  const timeline: RevisionTimelineEntry[] = revisions.map((r) => ({
    id: r.id,
    revisionNo: r.revision_no,
    revisionTiming:
      r.id === currentRev?.id
        ? isFutureDate(r.effective_from)
          ? ("FUTURE" as const)
          : ("CURRENT" as const)
        : ("HISTORICAL" as const),
    timingLabel:
      r.id === currentRev?.id
        ? isFutureDate(r.effective_from)
          ? "待生效"
          : "当前生效"
        : "已结束",
    nameSnapshot: r.name,
    actor: "—",
    effectiveFrom: r.effective_from,
    effectiveTo: r.effective_to ?? undefined,
    changeReason: "—",
    isCurrent: r.id === currentRev?.id,
    lifecycleAtRevision: asLifecycle(r.status),
  }))

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
      effectiveTo: currentRev?.effective_to ?? undefined,
      changeReason: "—",
      actor: "—",
      fields: row.keyFacts.map((f) => ({ label: f.label, value: f.value })),
    },
  })
}

async function centerSellable(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const items = await fetchAllPages<SellableSkuDto>("/admin/sellable-skus", {})
  const item = items.find((candidate) => candidate.sku_id === stableId)
  if (!item) return null
  const row = mapSkuAsSellable(item)
  return baseCenter("sellable-items", row)
}

async function centerVoucher(
  stableId: string
): Promise<MasterDataCenterView | null> {
  const profiles = await fetchAllPages<VoucherCategoryProfileDto>(
    "/admin/voucher-category-profiles",
    {}
  )
  // stableId 为 SKU 身份；兼容旧链接仍按 profile.id 查找。
  const matched = profiles.filter(
    (p) => p.sku_id === stableId || p.id === stableId
  )
  if (matched.length === 0) return null
  const profile = matched.reduce((best, cur) =>
    cur.revision_no > best.revision_no ? cur : best
  )
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

  const profile = detail.current_profile
  const contacts = detail.contacts
  const banks = detail.bank_accounts
  const taxProfiles = detail.tax_profiles
  const capabilities = detail.capabilities
  const qualifications = detail.qualifications
  const ratings = detail.ratings
  const profiles = detail.commercial_profiles
  const partyName =
    detail.legal_name ||
    detail.short_name ||
    detail.party_no ||
    detail.supplier_no
  const row = mapSupplierRow(detail, partyName, profile)

  const contact = pickDefaultOrFirst(contacts)
  const bank = pickDefaultOrFirst(banks)
  const taxProfile = pickDefaultOrFirst(taxProfiles)
  const sortedRatings = [...ratings].sort(
    (a, b) => (b.revision_no ?? 0) - (a.revision_no ?? 0)
  )
  const rating = sortedRatings[0]
  const initialRating = [...sortedRatings]
    .reverse()
    .find((item) => item.initial_score != null)
  const invoiceTaxRatePercent = taxRatePercent(profile?.invoice_tax_rate)

  const capabilityLabels = capabilities
    .map((c) => capabilityLabel(c.capability_code))
    .filter(Boolean)
    .join("、")
  const capabilityCodeById = new Map(
    capabilities.map((capability) => [capability.id, capability.capability_code]),
  )
  const qualificationCapabilityCodes = Object.fromEntries(
    qualifications.map((qualification) => [
        `${qualification.qualification_type}::${qualification.certificate_no}`,
        qualification.capability_ids.flatMap((id) => {
          const code = capabilityCodeById.get(id)
          return code ? [code] : []
        }),
      ]),
  )

  // 经营类目：商务快照编码；兼容早期写入 capability.fulfillment_note 的数据
  const businessCategory =
    parseBusinessCategoryFromSnapshot(profile?.payment_term_snapshot) ||
    capabilities.map((c) => c.fulfillment_note?.trim()).find(Boolean) ||
    ""

  const qualByType = (type: string) =>
    qualifications.find((q) => q.qualification_type === type)

  // 资质附件：解析 asset → 文件清单（fileName/assetId/url），供回显链接与编辑回填
  const qualGroups = new Map<string, SupplierQualificationDto[]>()
  for (const q of qualifications) {
    const list = qualGroups.get(q.qualification_type) ?? []
    list.push(q)
    qualGroups.set(q.qualification_type, list)
  }
  const qualAssets = await resolveMediaAssets(
    qualifications
      .map((q) => q.attachment_id)
      .filter((id): id is string => Boolean(id?.trim())),
  )
  const qualFieldEntries = (
    type: string,
  ): { fileName: string; assetId: string; url: string }[] =>
    (qualGroups.get(type) ?? []).flatMap((q) => {
      const asset = q.attachment_id ? qualAssets.get(q.attachment_id) : null
      if (!q.attachment_id) return []
      return [
        {
          fileName: asset?.file_name ?? q.certificate_no,
          assetId: q.attachment_id,
          url: asset?.public_url ?? "",
        },
      ]
    })
  const qualFileNames = (type: string): string =>
    qualFieldEntries(type)
      .map((entry) => entry.fileName)
      .join(", ")

  const contractQual = qualByType("contract")
  const authQual = qualByType("authorization")

  // 标签必须与 RESOURCE_FIELDS.suppliers / masterDataCopy 一致，供编辑回填
  const facts = factsOf(
    fact("供应商编号", detail.supplier_no),
    fact("企业主体", partyName),
    fact("统一社会信用代码", detail.unified_credit_code),
    fact("联系人", contact?.contact_name),
    // mobile 不在列表契约中；telephone 若创建时同步写入可回显
    fact("联系电话", contact?.telephone),
    fact("结算方式", settlementLabel(profile?.settlement_mode)),
    fact("发票类型", invoiceLabel(profile?.invoice_type)),
    fact("发票税点", invoiceTaxRatePercent),
    fact("能力", capabilityLabels),
    fact("经营类目", businessCategory || null),
    fact("公司签约主体", profile?.signing_entity_party_id),
    fact("公司付款主体", profile?.payment_entity_party_id),
    // 标签必须与 masterDataCopy / RESOURCE_FIELDS.suppliers 完全一致
    fact("资质附件", qualFileNames("certificate") || null),
    fact("合同编号", contractQual?.certificate_no),
    fact("合同有效期起", contractQual?.valid_from),
    fact("合同有效期止", contractQual?.valid_to),
    fact("合同文件", qualFileNames("contract") || null),
    fact("授权书文件", qualFileNames("authorization") || null),
    fact("授权书有效期起", authQual?.valid_from),
    fact("授权书有效期止", authQual?.valid_to),
    fact("食品经营许可证", qualFileNames("food_license") || null),
    fact("供应商法人身份证", qualFileNames("legal_person_id") || null),
    fact("税号", taxProfile?.tax_no),
    fact("开户银行", bank?.bank_name),
    // 银行账号明文不在列表契约中，无法回显
    fact("供应商评级", ratingLabel(rating?.rating)),
    fact(
      "合作期初评分",
      initialRating?.initial_score != null
        ? String(initialRating.initial_score)
        : null
    ),
    fact(
      "合作中评分",
      rating?.current_score != null ? String(rating.current_score) : null
    )
  )

  // 展示用摘要（含无值占位），与编辑 fields 分离
  const displayFacts = [
    { label: "供应商编号", value: detail.supplier_no },
    { label: "企业主体", value: partyName || "—" },
    { label: "联系人", value: contact?.contact_name || "—" },
    { label: "联系电话", value: contact?.telephone || "—" },
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
      value: invoiceTaxRatePercent ? `${invoiceTaxRatePercent}%` : "—",
    },
    { label: "能力", value: capabilityLabels || "—" },
    {
      label: "资质",
      value:
        qualifications.length > 0 ? `${qualifications.length} 项` : "—",
    },
    {
      label: "供应商评级",
      value: ratingLabel(rating?.rating) || "—",
    },
    {
      label: "税号",
      value: taxProfile?.tax_no ?? "—",
    },
    { label: "开户银行", value: bank?.bank_name || "—" },
  ]

  const timeline: RevisionTimelineEntry[] = profiles.map((p, index) => ({
    id: p.id,
    revisionNo: p.revision_no,
    revisionTiming: index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
    timingLabel: index === 0 ? "当前生效" : "已结束",
    nameSnapshot: partyName,
    actor: "—",
    effectiveFrom: tsToIso(p.created_at).slice(0, 10),
    changeReason: p.change_reason,
    isCurrent: index === 0,
    lifecycleAtRevision: asLifecycle(detail.status),
  }))

  return baseCenter("suppliers", row, {
    partyLockVersion: detail.party_version ?? undefined,
    supplierQualificationCapabilityCodes: qualificationCapabilityCodes,
    resourceFacts: displayFacts,
    currentRevision: {
      revisionId: profile?.id ?? detail.id,
      revisionNo: profile?.revision_no ?? detail.version,
      name: partyName,
      effectiveFrom: tsToIso(
        profile?.created_at ?? detail.created_at
      ).slice(0, 10),
      changeReason: profile?.change_reason ?? "—",
      actor: "—",
      // 编辑回填专用：完整字段 + 真实值（无「—」占位）
      fields: facts,
    },
    mediaAssets: {
      qualification: qualFieldEntries("certificate"),
      contractFile: qualFieldEntries("contract"),
      authorizationFile: qualFieldEntries("authorization"),
      foodLicense: qualFieldEntries("food_license"),
      legalPersonIdCard: qualFieldEntries("legal_person_id"),
    },
    revisionTimeline:
      timeline.length > 0
        ? timeline
        : baseCenter("suppliers", row).revisionTimeline,
    sensitiveFields: detail.sensitive_fields.map((field) => ({
      label: field.label,
      maskedValue: field.masked_value,
      revealToken: field.reveal_token,
      visibility: "masked" as const,
    })),
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
      logo_file_asset_id: fields.logoAssetId || null,
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

function parseQuantityScale(raw: string | undefined): number | null {
  const value = Number((raw ?? "").trim())
  if (!Number.isInteger(value) || value < 0 || value > 6) return null
  return value
}

async function createUnitOfMeasure(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  const fields = input.fields as UnitOfMeasureFields
  const quantityScale = parseQuantityScale(fields.quantityScale)
  if (quantityScale === null) {
    return {
      outcome: "blocked",
      code: "UNIT_QUANTITY_SCALE_INVALID",
      message: "数量小数位必须是 0–6 的整数。",
    }
  }
  if (!fields.code.trim()) {
    return {
      outcome: "blocked",
      code: "UNIT_CODE_REQUIRED",
      message: "请填写单位代码。",
    }
  }
  if (!fields.symbol.trim()) {
    return {
      outcome: "blocked",
      code: "UNIT_SYMBOL_REQUIRED",
      message: "请填写单位符号。",
    }
  }
  try {
    const created = await apiPost<UnitOfMeasureDto>("/admin/unit-of-measures", {
      unit_code: fields.code.trim(),
      name: input.name.trim(),
      symbol: fields.symbol.trim(),
      quantity_scale: quantityScale,
      status: "active",
    })
    return {
      outcome: "succeeded",
      stableId: created.id,
      stableNo: created.unit_code,
      revisionId: created.id,
      revisionNo: created.version,
      revisionState: "CURRENT",
      effectiveFrom: input.effectiveFrom,
      recordedAt: isoNow(),
      actor: "—",
      changeReason: input.changeReason || "新建",
      reference: `MD-CREATE-${created.unit_code}`,
      nextActions: ["查看详情", "更新资料"],
    }
  } catch (error) {
    return mapMutationError(error)
  }
}

/** SPU 媒体写入项：文件资产 + 展示顺序。 */
function mapProductMedia(
  names: readonly string[],
  assetIds: Readonly<Record<string, string>>,
): Array<{ file_asset_id: string; sort_order: number }> {
  return names
    .map((name, index) => ({
      file_asset_id: assetIds[name]?.trim() ?? "",
      sort_order: index,
    }))
    .filter((entry) => entry.file_asset_id)
}

function mapProductSkus(fields: ProductFields) {
  return fields.skus
    .filter((sku) => sku.lifecycleStatus === "ENABLED")
    .map((sku) => ({
      sku_id: sku.skuId || null,
      expected_sku_revision_id: sku.skuRevisionId || null,
      reenable: Boolean(sku.skuId && sku.requiresExplicitReenable),
      sku_no: sku.skuNo,
      base_unit_id: fields.baseUnitId,
      barcode: sku.barcode || null,
      main_image_asset_id: sku.mainImageAssetId || null,
      weight_kg: null,
      volume_m3: null,
      sales_visible_price_gross: sku.salePrice || null,
      market_price: sku.marketPrice || null,
      spec_entries: fields.specs.flatMap((spec, index) => {
        const attributeCode = spec.name.trim()
        const attributeValueCode = (sku.attributeValues[index] ?? "").trim()
        return attributeCode && attributeValueCode
          ? [{ attribute_code: attributeCode, attribute_value_code: attributeValueCode }]
          : []
      }),
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

  try {
    const created = await apiPost<ProductDto>("/admin/products", {
      change_reason: input.changeReason || "新建商品",
      product_no: fields.productNo.trim(),
      product_kind: fields.productKind,
      name: input.name.trim(),
      description: fields.description || null,
      specification: fields.specification || null,
      category_id: fields.categoryId,
      brand_id: fields.brandId,
      status: "active",
      effective_from: input.effectiveFrom,
      effective_to: input.effectiveTo || null,
      carousel_media: mapProductMedia(
        fields.carouselImages,
        fields.carouselFileAssetIds,
      ),
      detail_media: mapProductMedia(fields.detailImages, fields.detailFileAssetIds),
      skus: mapProductSkus(fields),
    })
    if (!created.current_revision_id) {
      throw new Error("商品创建成功但未返回当前修订，禁止伪造修订身份")
    }
    return {
      outcome: "succeeded",
      stableId: created.id,
      stableNo: created.product_no,
      revisionId: created.current_revision_id,
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

async function createVoucherCategory(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  const fields = input.fields as VoucherCategoryFields
  // 分类 / 品牌 / 单位均可省略：后端补齐共用卡券根分类、品牌「福尚云」、单位「张」。
  // 若调用方仍传入 categoryId / newCategory / brandId / baseUnitId，则原样转发覆盖默认。
  const body: Record<string, unknown> = {
    voucher_no: fields.voucherNo,
    name: input.name.trim(),
    description: (fields.description || input.name).trim(),
    specification: fields.specification || null,
    status: "active",
    effective_from: input.effectiveFrom || null,
    effective_to: input.effectiveTo || null,
  }
  if (fields.categoryId) {
    body.category_id = fields.categoryId
  } else if (fields.newCategoryCode && fields.newCategoryName) {
    body.new_category = {
      category_code: fields.newCategoryCode,
      parent_category_id: fields.newCategoryParentId || null,
      name: fields.newCategoryName,
    }
  }
  if (fields.brandId) {
    body.brand_id = fields.brandId
  }
  if (fields.baseUnitId) {
    body.sku = {
      base_unit_id: fields.baseUnitId,
      barcode: fields.barcode || null,
      weight_kg: null,
      volume_m3: null,
      sales_visible_price_gross: fields.salesVisiblePriceGross || null,
      market_price: fields.marketPrice || null,
    }
  }
  try {
    const created = await apiPost<VoucherCategoryProfileDto>(
      "/admin/voucher-categories",
      body
    )
    return {
      outcome: "succeeded",
      stableId: created.sku_id,
      stableNo: created.sku_no ?? fields.voucherNo,
      revisionId: created.id,
      revisionNo: created.revision_no,
      revisionState: isFutureDate(input.effectiveFrom) ? "FUTURE" : "CURRENT",
      effectiveFrom: input.effectiveFrom,
      recordedAt: isoNow(),
      actor: "—",
      changeReason: input.changeReason || "新建",
      reference: `MD-CREATE-VC-${fields.voucherNo}`,
      nextActions: ["返回列表"],
    }
  } catch (error) {
    return mapMutationError(error)
  }
}

type SupplierProfileQualificationInput = {
  qualification_type: string
  certificate_no: string
  issuer: null
  valid_from: string
  valid_to: string | null
  attachment_id: string | null
  capability_codes: string[]
}

/** 将页面文件字段转换为根级供应商资料命令中的结构化资质集合。 */
const buildSupplierProfileQualifications = (
  fields: SupplierFields,
  effectiveFrom: string,
  capabilityCodes: string[]
): SupplierProfileQualificationInput[] => {
  const result: SupplierProfileQualificationInput[] = []
  const pushFiles = (
    qualificationType: string,
    names: string[],
    assetIds: Readonly<Record<string, string>> | undefined,
    validFrom: string,
    validTo?: string,
    certificateNo?: string
  ) => {
    names.forEach((name, index) => {
      const attachmentId = assetIds?.[name]?.trim()
      if (!attachmentId) return
      const resolvedCertificateNo =
        certificateNo && index === 0 ? certificateNo : name
      const existingCodes =
        fields.qualificationCapabilityCodes?.[
          `${qualificationType}::${resolvedCertificateNo}`
        ]
      result.push({
        qualification_type: qualificationType,
        certificate_no: resolvedCertificateNo,
        issuer: null,
        valid_from: validFrom,
        valid_to: validTo || null,
        attachment_id: attachmentId,
        capability_codes: existingCodes
          ? existingCodes.filter((code) => capabilityCodes.includes(code))
          : capabilityCodes,
      })
    })
  }
  pushFiles(
    "certificate",
    parseMediaList(fields.qualification),
    fields.qualificationFileAssetIds,
    effectiveFrom
  )
  pushFiles(
    "contract",
    parseMediaList(fields.contractFile).slice(0, 1),
    fields.contractFileAssetIds,
    fields.contractValidFrom || effectiveFrom,
    fields.contractValidTo,
    fields.contractNo?.trim() || "CONTRACT"
  )
  pushFiles(
    "authorization",
    parseMediaList(fields.authorizationFile),
    fields.authorizationFileAssetIds,
    fields.authorizationValidFrom || effectiveFrom,
    fields.authorizationValidTo
  )
  pushFiles(
    "food_license",
    parseMediaList(fields.foodLicense),
    fields.foodLicenseFileAssetIds,
    effectiveFrom
  )
  pushFiles(
    "legal_person_id",
    parseMediaList(fields.legalPersonIdCard),
    fields.legalPersonIdCardFileAssetIds,
    effectiveFrom
  )
  return result
}

async function createSupplier(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  const fields = input.fields as SupplierFields
  const capabilityCodes = (fields.capability ?? "")
    .split(/[、,，]/)
    .map((value) => capabilityToBackend(value.trim()))
    .filter((value): value is string => Boolean(value))
  const effectiveFrom = input.effectiveFrom || todayDateOnly()
  if (!fields.signingEntity?.trim() || !fields.paymentEntity?.trim()) {
    return {
      outcome: "blocked",
      code: "SUPPLIER_INTERNAL_PARTY_REQUIRED",
      message: "请选择公司签约主体和公司付款主体。",
    }
  }
  try {
    const created = await apiPost<SupplierProfileMutationDto>(
      "/admin/supplier-profiles",
      {
        idempotency_key: input.idempotencyKey,
        party_no: genBusinessCode("PTY"),
        supplier_no: genBusinessCode("SUP"),
        expected_party_version: null,
        expected_supplier_version: null,
        legal_name: fields.company || input.name.trim(),
        short_name: input.name.trim(),
        unified_credit_code: fields.creditCode?.trim() || null,
        contact:
          fields.contactName?.trim() && fields.contactPhone?.trim()
            ? {
                contact_name: fields.contactName.trim(),
                mobile: fields.contactPhone.trim(),
                telephone: null,
                email: null,
              }
            : null,
        clear_contact: false,
        address: fields.address?.trim()
          ? {
              address: fields.address.trim(),
              contact_name: fields.contactName?.trim() || null,
            }
          : null,
        clear_address: false,
        tax_no: fields.taxNo?.trim() || null,
        clear_tax_profile: false,
        bank_account:
          fields.bankName?.trim() && fields.bankAccount?.trim()
            ? {
                bank_name: fields.bankName.trim(),
                account_number: fields.bankAccount.trim(),
              }
            : null,
        clear_bank_account: false,
        settlement_mode: settlementToBackend(fields.settlement),
        reconciliation_cycle: "monthly",
        payment_term_snapshot: buildPaymentTermSnapshot(
          fields.settlement,
          fields.businessCategory,
        ),
        invoice_type: invoiceToBackend(fields.invoiceType),
        invoice_tax_rate: normalizeTaxRate(fields.invoiceTaxRate),
        signing_entity_party_id: fields.signingEntity.trim(),
        payment_entity_party_id: fields.paymentEntity.trim(),
        capability_codes: capabilityCodes,
        qualifications: buildSupplierProfileQualifications(
          fields,
          effectiveFrom,
          capabilityCodes,
        ),
        rating:
          fields.supplierRating || fields.currentScore || fields.initialScore
            ? {
                initial_score: parseScore100(fields.initialScore) ?? null,
                rating: ratingToBackend(fields.supplierRating),
                current_score: parseScore100(fields.currentScore) ?? 0,
                valid_from: effectiveFrom,
              }
            : null,
        effective_from: effectiveFrom,
        change_reason: input.changeReason || "新建",
      },
    )

    return {
      outcome: "succeeded",
      stableId: created.supplier_id,
      stableNo: created.supplier_no,
      revisionId: created.revision_id,
      revisionNo: created.revision_no,
      revisionState: "CURRENT",
      effectiveFrom: created.effective_from,
      recordedAt: tsToIso(created.recorded_at),
      actor: "—",
      changeReason: created.change_reason,
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
    case "unit-of-measures":
      rows = await listUnitOfMeasures(query)
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

/** 按稳定 SKU 查询正式供给，返回当前启用供给的去重供应商数量。 */
export async function fetchSkuSupplierCounts(
  skuIds: readonly string[]
): Promise<Map<string, number>> {
  const uniqueIds = [...new Set(skuIds.filter(Boolean))]
  const entries = await Promise.all(
    uniqueIds.map(async (skuId) => {
      const offerings = await fetchAllPages<SupplierOfferingSummaryDto>(
        "/admin/supplier-offerings",
        { sku_id: skuId }
      )
      const supplierIds = new Set(
        offerings
          .filter(
            (offering) =>
              offering.status === "ACTIVE" &&
              Boolean(offering.current_revision_id)
          )
          .map((offering) => offering.supplier_id)
      )
      return [skuId, supplierIds.size] as const
    })
  )
  return new Map(entries)
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
    case "unit-of-measures":
      return centerUnitOfMeasure(stableId)
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
    case "unit-of-measures":
      return createUnitOfMeasure(input)
    case "products":
      return createProduct(input)
    case "voucher-categories":
      return createVoucherCategory(input)
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
        const fields = input.fields as BrandFields
        const updated = await apiPut<ProductBrandDto>(
          `/admin/product-brands/${input.stableId}`,
          {
            version: input.expectedLockVersion,
            name: input.name.trim(),
            logo_file_asset_id: fields.logo
              ? fields.logoAssetId || null
              : null,
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
      case "unit-of-measures": {
        const fields = input.fields as UnitOfMeasureFields
        const quantityScale = parseQuantityScale(fields.quantityScale)
        if (quantityScale === null) {
          return {
            outcome: "blocked",
            code: "UNIT_QUANTITY_SCALE_INVALID",
            message: "数量小数位必须是 0–6 的整数。",
          }
        }
        if (!fields.symbol.trim()) {
          return {
            outcome: "blocked",
            code: "UNIT_SYMBOL_REQUIRED",
            message: "请填写单位符号。",
          }
        }
        const updated = await apiPut<UnitOfMeasureDto>(
          `/admin/unit-of-measures/${input.stableId}`,
          {
            version: input.expectedLockVersion,
            name: input.name.trim(),
            symbol: fields.symbol.trim(),
            quantity_scale: quantityScale,
          }
        )
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.unit_code,
          revisionId: updated.id,
          revisionNo: updated.version,
          revisionState: "CURRENT",
          effectiveFrom: input.effectiveFrom,
          recordedAt: isoNow(),
          actor: "—",
          changeReason: input.changeReason,
          reference: `MD-REV-${updated.unit_code}-v${updated.version}`,
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
            change_reason: input.changeReason,
            name: input.name.trim(),
            description: fields.description || null,
            specification: fields.specification || null,
            category_id: fields.categoryId,
            brand_id: fields.brandId,
            status:
              fields.lifecycleStatus === "DISABLED" ? "disabled" : "active",
            effective_from: input.effectiveFrom,
            effective_to: input.effectiveTo || null,
            carousel_media: mapProductMedia(
              fields.carouselImages,
              fields.carouselFileAssetIds,
            ),
            detail_media: mapProductMedia(
              fields.detailImages,
              fields.detailFileAssetIds,
            ),
            skus: mapProductSkus(fields),
          }
        )
        if (!updated.current_revision_id) {
          throw new Error("商品更新成功但未返回当前修订，禁止伪造修订身份")
        }
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.product_no,
          revisionId: updated.current_revision_id,
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
        const fields = input.fields as SupplierFields
        const capabilityCodes = (fields.capability ?? "")
          .split(/[、,，]/)
          .map((value) => capabilityToBackend(value.trim()))
          .filter((value): value is string => Boolean(value))
        const effectiveFrom = input.effectiveFrom || todayDateOnly()
        if (
          input.expectedPartyVersion == null ||
          !fields.signingEntity?.trim() ||
          !fields.paymentEntity?.trim()
        ) {
          return {
            outcome: "blocked",
            code: "SUPPLIER_PROFILE_REQUIRED_CONTEXT",
            message: "供应商版本或签约、付款主体缺失，请刷新后重试。",
          }
        }
        const updated = await apiPut<SupplierProfileMutationDto>(
          `/admin/supplier-profiles/${input.stableId}`,
          {
            idempotency_key: input.idempotencyKey,
            party_no: null,
            supplier_no: null,
            expected_party_version: input.expectedPartyVersion,
            expected_supplier_version: input.expectedLockVersion,
            legal_name: fields.company || input.name.trim(),
            short_name: input.name.trim(),
            unified_credit_code: fields.creditCode?.trim() || null,
            contact:
              fields.contactName?.trim() && fields.contactPhone?.trim()
                ? {
                    contact_name: fields.contactName.trim(),
                    mobile: fields.contactPhone.trim(),
                    telephone: null,
                    email: null,
                  }
                : null,
            clear_contact: fields.clearContact === true,
            address: fields.address?.trim()
              ? {
                  address: fields.address.trim(),
                  contact_name: fields.contactName?.trim() || null,
                }
              : null,
            clear_address: fields.clearAddress === true,
            tax_no: fields.taxNo?.trim() || null,
            clear_tax_profile: fields.clearTaxProfile === true,
            bank_account:
              fields.bankName?.trim() && fields.bankAccount?.trim()
                ? {
                    bank_name: fields.bankName.trim(),
                    account_number: fields.bankAccount.trim(),
                  }
                : null,
            clear_bank_account: fields.clearBankAccount === true,
            settlement_mode: settlementToBackend(fields.settlement),
            reconciliation_cycle: "monthly",
            payment_term_snapshot: buildPaymentTermSnapshot(
              fields.settlement,
              fields.businessCategory,
            ),
            invoice_type: invoiceToBackend(fields.invoiceType),
            invoice_tax_rate: normalizeTaxRate(fields.invoiceTaxRate),
            signing_entity_party_id: fields.signingEntity.trim(),
            payment_entity_party_id: fields.paymentEntity.trim(),
            capability_codes: capabilityCodes,
            qualifications: buildSupplierProfileQualifications(
              fields,
              effectiveFrom,
              capabilityCodes,
            ),
            rating:
              fields.supplierRating || fields.currentScore || fields.initialScore
                ? {
                    initial_score: parseScore100(fields.initialScore) ?? null,
                    rating: ratingToBackend(fields.supplierRating),
                    current_score: parseScore100(fields.currentScore) ?? 0,
                    valid_from: effectiveFrom,
                  }
                : null,
            effective_from: effectiveFrom,
            change_reason: input.changeReason,
          },
        )
        return {
          outcome: "succeeded",
          stableId: updated.supplier_id,
          stableNo: updated.supplier_no,
          revisionId: updated.revision_id,
          revisionNo: updated.revision_no,
          revisionState: "CURRENT",
          effectiveFrom: updated.effective_from,
          recordedAt: tsToIso(updated.recorded_at),
          actor: "—",
          changeReason: updated.change_reason,
          reference: `MD-REV-${updated.supplier_no}-v${updated.revision_no}`,
          nextActions: ["查看变更历史", "返回列表"],
        }
      }
      case "sellable-items":
        return {
          outcome: "blocked",
          code: "SELLABLE_NOT_WRITABLE",
          message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
        }
      case "voucher-categories": {
        const fields = input.fields as VoucherCategoryFields
        try {
          const updated = await apiPut<VoucherCategoryProfileDto>(
            `/admin/voucher-categories/${input.stableId}`,
            {
              version: input.expectedLockVersion,
              name: input.name.trim(),
              description: (
                fields.description ||
                input.name
              ).trim(),
              effective_from: input.effectiveFrom || null,
              effective_to: input.effectiveTo || null,
            }
          )
          return {
            outcome: "succeeded",
            stableId: updated.sku_id,
            stableNo: updated.sku_no ?? fields.voucherNo ?? input.stableId,
            revisionId: updated.id,
            revisionNo: updated.revision_no,
            revisionState: isFutureDate(input.effectiveFrom)
              ? "FUTURE"
              : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "更新",
            reference: `MD-REV-VC-${updated.sku_no ?? input.stableId}-v${updated.revision_no}`,
            nextActions: ["返回列表"],
          }
        } catch (error) {
          return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
          })
        }
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
      case "unit-of-measures": {
        const updated = await apiPut<UnitOfMeasureDto>(
          `/admin/unit-of-measures/${input.stableId}`,
          {
            version: input.expectedLockVersion,
            status: "disabled",
          }
        )
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.unit_code,
          revisionId: updated.id,
          revisionNo: updated.version,
          revisionState: "CURRENT",
          effectiveFrom: input.effectiveFrom,
          recordedAt: isoNow(),
          actor: "—",
          changeReason: input.changeReason,
          reference: `MD-DIS-${updated.unit_code}`,
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
            change_reason: input.changeReason,
            name: center.name,
            description: detail?.description || null,
            specification: detail?.specification || null,
            category_id: detail?.categoryId || "",
            brand_id: detail?.brandId || "",
            status: "disabled",
            effective_from: input.effectiveFrom,
            effective_to: center.currentRevision.effectiveTo || null,
            carousel_media: mapProductMedia(
              detail?.carouselImages ?? [],
              detail?.carouselFileAssetIds ?? {}
            ),
            detail_media: mapProductMedia(
              detail?.detailImages ?? [],
              detail?.detailFileAssetIds ?? {}
            ),
            skus: detail
              ? mapProductSkus({
                  ...detail,
                  productKind: center.productKind ?? "",
                })
              : [],
          }
        )
        if (!updated.current_revision_id) {
          throw new Error("商品停用成功但未返回当前修订，禁止伪造修订身份")
        }
        return {
          outcome: "succeeded",
          stableId: updated.id,
          stableNo: updated.product_no,
          revisionId: updated.current_revision_id,
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
          code: "VOUCHER_NO_DISABLE",
          message: "卡券类目不支持停用。",
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

/** 使用短期令牌揭示供应商敏感字段；服务端再次执行权限校验并记录审计。 */
export async function revealMasterDataSensitive(
  revealToken: string
): Promise<string> {
  const result = await apiPost<{ value: string }>(
    "/admin/supplier-sensitive-fields/reveal",
    { reveal_token: revealToken },
  )
  return result.value
}

// Re-export pure display helpers used by pages (stable import path via queries)
