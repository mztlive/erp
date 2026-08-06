/**
 * W22 · 商品发布 · 真实 HTTP 适配层。
 * 保持对外导出签名稳定；后端 Page/DTO 在此映射为 feature 视图类型。
 */

import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import type {
  DeliveryStatus,
  ManualPauseCommand,
  ManualPauseResult,
  ProductPublicationListQuery,
  ProductPublicationListResult,
  ProductPublicationRow,
  ProductPublicationView,
  PublicationStatus,
  PublishRevisionCommand,
  PublishRevisionResult,
  RetryDeliveryCommand,
  RetryDeliveryResult,
  SaleStatus,
} from "@/features/product-publications/types"
import {
  DELIVERY_STATUS_LABEL,
  DELIVERY_STATUS_TONE,
  PUBLICATION_STATUS_LABEL,
  PUBLICATION_STATUS_TONE,
  SALE_STATUS_LABEL,
} from "@/features/product-publications/types"

// ─── Backend wire types ───────────────────────────────────────────────────────

type BackendPublication = {
  id: string
  sku_id: string
  target_mall_id: string
  status: string
  current_revision_id?: string | null
  version: number
  created_at: number
}

type BackendRevision = {
  id: string
  product_publication_id: string
  revision_no: number
  name: string
  sale_status: string
  sales_price_gross: string
  valid_from: number
  valid_to?: number | null
  version: number
  created_at: number
}

type BackendDelivery = {
  id: string
  publication_revision_id: string
  target_mall_id: string
  delivery_status: string
  attempt_count: number
  mall_version?: string | null
  error_code?: string | null
  version: number
  created_at: number
}

type BackendDeliveryResult = {
  delivery_id: string
  delivery_status: string
  inbox_message_id: string
  error_task_id?: string | null
  mall_version?: string | null
  publication_version: number
}

type BackendMedia = {
  id: string
  product_publication_revision_id: string
  file_asset_id: string
  media_role: string
  sort_no: number
  alt_text?: string | null
}

type SourceSystem = {
  id: string
  code: string
  name: string
  system_type?: string
  status?: string
}

// ─── Constants re-exported for list page ──────────────────────────────────────

/** 商城选项：运行时从 source-systems 填充；初始为空，列表接口内补齐。 */
export let MALLS: Array<{ id: string; name: string }> = []

// ─── Helpers ──────────────────────────────────────────────────────────────────

function secsToIso(secs?: number | null): string {
  if (secs == null || secs <= 0) return new Date(0).toISOString()
  return new Date(secs * 1000).toISOString()
}

function mapPublicationStatus(raw: string): PublicationStatus {
  switch (raw) {
    case "draft":
      return "DRAFT"
    case "pending_publish":
      return "PENDING_PUBLISH"
    case "mall_effective":
      return "MALL_LIVE"
    case "paused":
      return "PAUSED"
    case "expired":
      return "INVALID"
    default:
      return "DRAFT"
  }
}

function toBackendPublicationStatus(s: string): string | undefined {
  const table: Record<string, string> = {
    DRAFT: "draft",
    PENDING_PUBLISH: "pending_publish",
    MALL_LIVE: "mall_effective",
    PAUSED: "paused",
    SAFETY_PAUSED: "paused",
    INVALID: "expired",
  }
  return table[s]
}

function mapDeliveryStatus(raw: string): DeliveryStatus {
  switch (raw) {
    case "pending_send":
      return "PENDING_SEND"
    case "retrying":
      return "RETRYING"
    case "confirmed":
      return "ACKED"
    case "failed":
      return "FAILED"
    case "manual":
      return "HANDOFF"
    case "sending":
      return "SENDING"
    default:
      return "PENDING_SEND"
  }
}

function mapSaleStatus(raw: string): SaleStatus {
  switch (raw) {
    case "on_sale":
      return "ON_SALE"
    case "off_sale":
      return "OFF_SALE"
    case "pause_order":
      return "PAUSED"
    default:
      return "ON_SALE"
  }
}

function toBackendSaleStatus(s: SaleStatus): string {
  switch (s) {
    case "ON_SALE":
      return "on_sale"
    case "OFF_SALE":
      return "off_sale"
    case "PAUSED":
      return "pause_order"
  }
}

function toBackendMediaRole(role: string): string {
  switch (role) {
    case "MAIN":
      return "main"
    case "CAROUSEL":
      return "carousel"
    case "DETAIL":
      return "detail"
    default:
      return role.toLowerCase()
  }
}

async function loadMalls(): Promise<Array<{ id: string; name: string }>> {
  try {
    const page = await apiGet<Page<SourceSystem>>("/admin/source-systems", {
      page: 1,
      page_size: 100,
      system_type: "MALL",
    })
    const list = page.items.map((s) => ({ id: s.id, name: s.name }))
    MALLS = list
    return list
  } catch {
    return MALLS
  }
}

function mallName(
  malls: Array<{ id: string; name: string }>,
  id: string
): string {
  return malls.find((m) => m.id === id)?.name ?? id
}

function emptyFixedOffering() {
  return {
    offeringRevisionId: "",
    supplierName: "—",
    availability: "UNKNOWN",
    availabilityLabel: "未返回",
    supplyPriceVisible: false as const,
  }
}

function rowFromPublication(
  pub: BackendPublication,
  rev: BackendRevision | undefined,
  delivery: BackendDelivery | undefined,
  malls: Array<{ id: string; name: string }>
): ProductPublicationRow {
  const status = mapPublicationStatus(pub.status)
  const delStatus = delivery
    ? mapDeliveryStatus(delivery.delivery_status)
    : undefined

  return {
    publicationId: pub.id,
    publicationCode: pub.id.slice(0, 12).toUpperCase(),
    skuId: pub.sku_id,
    skuCode: pub.sku_id,
    productName: rev?.name ?? pub.sku_id,
    specification: "",
    targetMallId: pub.target_mall_id,
    targetMallName: mallName(malls, pub.target_mall_id),
    publicationStatus: status,
    publicationStatusLabel: PUBLICATION_STATUS_LABEL[status],
    publicationStatusTone: PUBLICATION_STATUS_TONE[status],
    currentAckedRevisionId: pub.current_revision_id ?? undefined,
    latestRevisionId: rev?.id,
    latestRevisionNo: rev?.revision_no,
    hasPendingConfirmation: Boolean(
      rev &&
        pub.current_revision_id &&
        rev.id !== pub.current_revision_id
    ),
    salesPriceGross: rev?.sales_price_gross,
    fixedOffering: emptyFixedOffering(),
    latestDelivery: delivery
      ? {
          deliveryId: delivery.id,
          status: delStatus!,
          statusLabel: DELIVERY_STATUS_LABEL[delStatus!],
          statusTone: DELIVERY_STATUS_TONE[delStatus!],
          attemptCount: delivery.attempt_count,
          errorSummary: delivery.error_code ?? undefined,
        }
      : undefined,
    ownerLabel: "—",
    updatedAt: secsToIso(pub.created_at),
    allowedActions: ["OPEN_CENTER", "PUBLISH_REVISION", "MANUAL_PAUSE", "RETRY_DELIVERY"],
    actionBlockers: [],
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

export async function fetchPublicationList(
  query: ProductPublicationListQuery
): Promise<ProductPublicationListResult> {
  const malls = await loadMalls()
  const page = query.page ?? 1
  const pageSize = query.pageSize ?? 20

  const listQuery: Record<string, unknown> = {
    page,
    page_size: pageSize,
    sort_by: "updated_at",
    sort_dir: "desc",
  }
  if (query.skuId) listQuery.sku_id = query.skuId
  if (query.mallId) listQuery.target_mall_id = query.mallId
  if (query.publicationStatus && query.publicationStatus !== "all") {
    const mapped = toBackendPublicationStatus(query.publicationStatus)
    if (mapped) listQuery.status = mapped
  }

  const pageResult = await apiGet<Page<BackendPublication>>(
    "/admin/product-publications",
    listQuery
  )

  // 投递列表（用于 latest delivery 投影）
  const deliveryPage = await apiGet<Page<BackendDelivery>>(
    "/admin/product-publication-deliveries",
    { page: 1, page_size: 100 }
  ).catch(() => ({
    items: [] as BackendDelivery[],
    total: 0,
    page: 1,
    page_size: 100,
  }))

  const rows: ProductPublicationRow[] = []
  for (const pub of pageResult.items) {
    const revisions = await apiGet<BackendRevision[]>(
      `/admin/product-publications/${encodeURIComponent(pub.id)}/revisions`
    ).catch(() => [] as BackendRevision[])
    const latest = revisions[0]
    const delivery = deliveryPage.items.find(
      (d) =>
        d.publication_revision_id === latest?.id ||
        d.target_mall_id === pub.target_mall_id
    )
    rows.push(rowFromPublication(pub, latest, delivery, malls))
  }

  // 客户端补筛（后端未提供 metric / deliveryStatus / q）
  let filtered = rows
  if (query.q?.trim()) {
    const q = query.q.trim().toUpperCase()
    filtered = filtered.filter(
      (r) =>
        r.publicationCode.toUpperCase().includes(q) ||
        r.skuCode.toUpperCase().includes(q) ||
        r.productName.toUpperCase().includes(q) ||
        r.targetMallName.toUpperCase().includes(q) ||
        r.publicationId.toUpperCase().includes(q)
    )
  }
  if (query.deliveryStatus && query.deliveryStatus !== "all") {
    if (query.deliveryStatus === "pending_confirm") {
      filtered = filtered.filter((r) => {
        const s = r.latestDelivery?.status
        return s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
      })
    } else if (query.deliveryStatus === "failed") {
      filtered = filtered.filter((r) => r.latestDelivery?.status === "FAILED")
    } else if (query.deliveryStatus === "handoff") {
      filtered = filtered.filter((r) => r.latestDelivery?.status === "HANDOFF")
    } else if (query.deliveryStatus === "acked") {
      filtered = filtered.filter((r) => r.latestDelivery?.status === "ACKED")
    }
  }
  if (query.metric && query.metric !== "all") {
    if (query.metric === "pending_confirm") {
      filtered = filtered.filter((r) => {
        const s = r.latestDelivery?.status
        return s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
      })
    } else if (query.metric === "failed_handoff") {
      filtered = filtered.filter(
        (r) =>
          r.latestDelivery?.status === "FAILED" ||
          r.latestDelivery?.status === "HANDOFF"
      )
    } else if (query.metric === "mall_live") {
      filtered = filtered.filter((r) => r.publicationStatus === "MALL_LIVE")
    } else if (query.metric === "paused") {
      filtered = filtered.filter(
        (r) =>
          r.publicationStatus === "PAUSED" ||
          r.publicationStatus === "SAFETY_PAUSED"
      )
    } else if (query.metric === "pending_publish") {
      filtered = filtered.filter(
        (r) => r.publicationStatus === "PENDING_PUBLISH"
      )
    }
  }

  // 默认排除失效
  if (query.publicationStatus !== "INVALID") {
    filtered = filtered.filter((r) => r.publicationStatus !== "INVALID")
  }

  // 指标：仅基于本页 — backend_gap（无汇总端点）
  const metrics = {
    pendingPublish: filtered.filter(
      (r) => r.publicationStatus === "PENDING_PUBLISH"
    ).length,
    pendingConfirm: filtered.filter((r) => {
      const s = r.latestDelivery?.status
      return s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
    }).length,
    failedOrHandoff: filtered.filter(
      (r) =>
        r.latestDelivery?.status === "FAILED" ||
        r.latestDelivery?.status === "HANDOFF"
    ).length,
    mallLive: filtered.filter((r) => r.publicationStatus === "MALL_LIVE")
      .length,
    paused: filtered.filter(
      (r) =>
        r.publicationStatus === "PAUSED" ||
        r.publicationStatus === "SAFETY_PAUSED"
    ).length,
  }

  const hasFilters = Boolean(
    query.q?.trim() ||
      query.mallId ||
      query.skuId ||
      query.supplierOfferingRevisionId ||
      (query.publicationStatus && query.publicationStatus !== "all") ||
      (query.deliveryStatus && query.deliveryStatus !== "all") ||
      (query.metric && query.metric !== "all")
  )

  return {
    items: filtered,
    total: pageResult.total,
    page: pageResult.page,
    pageSize: pageResult.page_size,
    metrics,
    permissionVersion: "pv-live",
    dataScopeVersion: "ds-live",
    queriedAt: secsToIso(
      Math.max(0, ...pageResult.items.map((p) => p.created_at))
    ),
    creationBlocker: {
      code: "PUBLICATION_IDENTITY_POLICY_UNCONFIRMED",
      message:
        "新建发布身份策略尚未在后端确认；列表/详情/修订/投递已接入真实接口。",
    },
    filterSummary: `${filtered.length} 条`,
    emptyReason:
      filtered.length === 0
        ? hasFilters
          ? "FILTER_NO_RESULT"
          : "NO_DATA"
        : undefined,
    resolvedFilters: {
      skuCode: query.skuId,
    },
  }
}

export async function fetchPublicationDetail(
  publicationId: string,
  revisionId?: string
): Promise<ProductPublicationView | null> {
  const malls = await loadMalls()

  let pub: BackendPublication
  try {
    pub = await apiGet<BackendPublication>(
      `/admin/product-publications/${encodeURIComponent(publicationId)}`
    )
  } catch (err) {
    const e = err as { kind?: string; status?: number }
    if (e?.kind === "Http" && e.status === 404) return null
    throw err
  }

  const revisions = await apiGet<BackendRevision[]>(
    `/admin/product-publications/${encodeURIComponent(publicationId)}/revisions`
  ).catch(() => [] as BackendRevision[])

  const selected =
    revisions.find((r) => r.id === revisionId) ??
    revisions.find((r) => r.id === pub.current_revision_id) ??
    revisions[0]

  if (!selected) {
    // 无修订时仍返回骨架，避免整页 null
    const status = mapPublicationStatus(pub.status)
    return {
      identity: {
        publicationId: pub.id,
        publicationCode: pub.id.slice(0, 12).toUpperCase(),
        skuId: pub.sku_id,
        skuCode: pub.sku_id,
        targetMallId: pub.target_mall_id,
        targetMallName: mallName(malls, pub.target_mall_id),
      },
      status,
      statusLabel: PUBLICATION_STATUS_LABEL[status],
      statusTone: PUBLICATION_STATUS_TONE[status],
      currentAckedRevisionId: pub.current_revision_id ?? undefined,
      selectedRevision: {
        revisionId: "",
        revisionNo: 0,
        skuRevisionId: "",
        supplierOfferingRevisionId: "",
        fixedOffering: emptyFixedOffering(),
        categoryId: "",
        categoryLabel: "—",
        name: "—",
        specification: "",
        salesDescription: "",
        minimumPurchaseQuantity: "1",
        salesPriceGross: "0",
        salesTaxRate: "0",
        baseUnitCode: "",
        salesRegionLabel: "—",
        saleStatus: "ON_SALE",
        saleStatusLabel: SALE_STATUS_LABEL.ON_SALE,
        productCapabilities: [],
        validFrom: secsToIso(pub.created_at),
        contentHash: "",
        media: [],
        createdAt: secsToIso(pub.created_at),
        createdBy: "—",
      },
      revisions: [],
      deliveries: [],
      publishGate: {
        kind: "READY",
        gateVersion: "1",
        submissionKind: "NORMAL",
        priceOrTaxChanged: false,
        policyVersion: "1",
        reviewDisposition: "NOT_REQUIRED",
      },
      freshness: {
        queriedAt: secsToIso(pub.created_at),
        integrationUpdatedAt: secsToIso(pub.created_at),
      },
      allowedActions: ["PUBLISH_REVISION"],
      actionBlockers: [],
      fieldPermissions: {},
      objectVersion: String(pub.version),
      ownerLabel: "—",
    }
  }

  const media = await apiGet<BackendMedia[]>(
    `/admin/product-publication-revisions/${encodeURIComponent(selected.id)}/media`
  ).catch(() => [] as BackendMedia[])

  const deliveryPage = await apiGet<Page<BackendDelivery>>(
    "/admin/product-publication-deliveries",
    {
      page: 1,
      page_size: 100,
      target_mall_id: pub.target_mall_id,
    }
  ).catch(() => ({
    items: [] as BackendDelivery[],
    total: 0,
    page: 1,
    page_size: 100,
  }))

  const revIds = new Set(revisions.map((r) => r.id))
  const deliveries = deliveryPage.items
    .filter((d) => revIds.has(d.publication_revision_id))
    .map((d) => {
      const rev = revisions.find((r) => r.id === d.publication_revision_id)
      const st = mapDeliveryStatus(d.delivery_status)
      return {
        deliveryId: d.id,
        revisionId: d.publication_revision_id,
        revisionNo: rev?.revision_no ?? 0,
        targetMallId: d.target_mall_id,
        status: st,
        statusLabel: DELIVERY_STATUS_LABEL[st],
        statusTone: DELIVERY_STATUS_TONE[st],
        attemptCount: d.attempt_count,
        lastAttemptAt: secsToIso(d.created_at),
        mallVersion: d.mall_version ?? undefined,
        errorCode: d.error_code ?? undefined,
        errorSummary: d.error_code ?? undefined,
      }
    })

  const saleStatus = mapSaleStatus(selected.sale_status)
  const status = mapPublicationStatus(pub.status)
  const latest = revisions[0]

  return {
    identity: {
      publicationId: pub.id,
      publicationCode: pub.id.slice(0, 12).toUpperCase(),
      skuId: pub.sku_id,
      skuCode: pub.sku_id,
      targetMallId: pub.target_mall_id,
      targetMallName: mallName(malls, pub.target_mall_id),
    },
    status,
    statusLabel: PUBLICATION_STATUS_LABEL[status],
    statusTone: PUBLICATION_STATUS_TONE[status],
    currentAckedRevisionId: pub.current_revision_id ?? undefined,
    latestRevisionId: latest?.id,
    latestRevisionNo: latest?.revision_no,
    selectedRevision: {
      revisionId: selected.id,
      revisionNo: selected.revision_no,
      skuRevisionId: "",
      supplierOfferingRevisionId: "",
      fixedOffering: emptyFixedOffering(),
      categoryId: "",
      categoryLabel: "—",
      name: selected.name,
      specification: "",
      salesDescription: "",
      minimumPurchaseQuantity: "1",
      salesPriceGross: String(selected.sales_price_gross ?? "0"),
      salesTaxRate: "0",
      baseUnitCode: "",
      salesRegionLabel: "—",
      saleStatus,
      saleStatusLabel: SALE_STATUS_LABEL[saleStatus],
      productCapabilities: [],
      validFrom: secsToIso(selected.valid_from),
      validTo: selected.valid_to
        ? secsToIso(selected.valid_to)
        : undefined,
      contentHash: selected.id,
      media: media.map((m) => ({
        fileAssetId: m.file_asset_id,
        mediaRole:
          m.media_role === "main"
            ? ("MAIN" as const)
            : m.media_role === "carousel"
              ? ("CAROUSEL" as const)
              : ("DETAIL" as const),
        sortNo: m.sort_no,
        altText: m.alt_text ?? "",
        thumbnailUrl: "",
        securityScanStatus: "PASSED" as const,
      })),
      createdAt: secsToIso(selected.created_at),
      createdBy: "—",
    },
    revisions: revisions.map((r) => {
      const delivery = deliveries.find((d) => d.revisionId === r.id)
      const ss = mapSaleStatus(r.sale_status)
      return {
        revisionId: r.id,
        revisionNo: r.revision_no,
        saleStatus: ss,
        saleStatusLabel: SALE_STATUS_LABEL[ss],
        createdAt: secsToIso(r.created_at),
        createdBy: "—",
        contentHash: r.id,
        deliverySummary: delivery
          ? `${delivery.statusLabel}${delivery.errorSummary ? ` · ${delivery.errorSummary}` : ""}`
          : "无发送",
        isMallAcked: r.id === pub.current_revision_id,
        isLatest: r.id === latest?.id,
      }
    }),
    deliveries,
    publishGate: {
      kind: "READY",
      gateVersion: String(pub.version),
      submissionKind: "NORMAL",
      priceOrTaxChanged: false,
      policyVersion: "1",
      reviewDisposition: "NOT_REQUIRED",
    },
    freshness: {
      queriedAt: secsToIso(pub.created_at),
      integrationUpdatedAt: secsToIso(selected.created_at),
    },
    allowedActions: [
      "PUBLISH_REVISION",
      "MANUAL_PAUSE",
      "RETRY_DELIVERY",
      "OPEN_CENTER",
    ],
    actionBlockers: [],
    fieldPermissions: {},
    objectVersion: String(pub.version),
    ownerLabel: "—",
  }
}

export async function publishRevision(
  command: PublishRevisionCommand
): Promise<PublishRevisionResult> {
  const content = command.content
  const revision = await apiPost<BackendRevision>(
    `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions`,
    {
      sku_revision_id: content.skuRevisionId,
      supplier_offering_revision_id: content.supplierOfferingRevisionId,
      category_id: content.categoryId,
      name: content.name,
      specification: content.specification || null,
      sales_description: content.salesDescription,
      minimum_purchase_quantity: content.minimumPurchaseQuantity,
      sales_price_gross: content.salesPriceGross,
      sales_tax_rate: content.salesTaxRate,
      base_unit_code: content.baseUnitCode,
      sales_region: content.salesRegion?.join(",") || null,
      sale_status: toBackendSaleStatus(content.saleStatus),
      product_capabilities: content.productCapabilities.map((c) =>
        c.toLowerCase()
      ),
      valid_from: Math.floor(new Date(content.validFrom).getTime() / 1000) || 1,
      valid_to: content.validTo
        ? Math.floor(new Date(content.validTo).getTime() / 1000)
        : null,
      media: content.media.map((m) => ({
        file_asset_id: m.fileAssetId,
        media_role: toBackendMediaRole(m.mediaRole),
        sort_no: m.sortNo,
        alt_text: m.altText || null,
      })),
    }
  )

  const delivery = await apiPost<BackendDeliveryResult>(
    `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions/${revision.revision_no}/deliver`,
    { idempotency_key: command.requestId }
  )

  return {
    status: "succeeded",
    operationId: delivery.inbox_message_id,
    publicationId: command.publicationId,
    revisionId: revision.id,
    revisionNo: revision.revision_no,
    deliveryId: delivery.delivery_id,
    deliveryStatus: "PENDING_SEND",
    committedAt: secsToIso(revision.created_at),
  }
}

export async function manualPausePublication(
  command: ManualPauseCommand
): Promise<ManualPauseResult> {
  await apiPut<BackendPublication>(
    `/admin/product-publications/${encodeURIComponent(command.publicationId)}`,
    {
      version: Number(command.expectedObjectVersion) || 1,
      status: "paused",
    }
  )

  return {
    status: "succeeded",
    revisionId: "",
    revisionNo: 0,
    deliveryId: "",
    committedAt: new Date().toISOString(),
  }
}

export async function retryDelivery(
  command: RetryDeliveryCommand
): Promise<RetryDeliveryResult> {
  // 重试：对当前发布的最新修订再次 deliver（幂等键 = requestId）
  const revisions = await apiGet<BackendRevision[]>(
    `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions`
  ).catch(() => [] as BackendRevision[])
  const latest = revisions[0]
  if (!latest) {
    return {
      status: "blocked",
      code: "NO_REVISION",
      message: "无可重试的发布修订",
    }
  }

  const result = await apiPost<BackendDeliveryResult>(
    `/admin/product-publications/${encodeURIComponent(command.publicationId)}/revisions/${latest.revision_no}/deliver`,
    { idempotency_key: command.requestId }
  )

  const st = mapDeliveryStatus(result.delivery_status)
  return {
    status: "succeeded",
    deliveryId: result.delivery_id,
    attemptCount: 1,
    deliveryStatus: st,
  }
}

export { PUBLICATION_STATUS_LABEL, PUBLICATION_STATUS_TONE }
export { DELIVERY_STATUS_LABEL, DELIVERY_STATUS_TONE, SALE_STATUS_LABEL }
