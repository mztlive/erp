/**
 * W22 商品发布 · session-mock API（queryFn / mutationFn）
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  ManualPauseCommand,
  ManualPauseResult,
  ProductPublicationListQuery,
  ProductPublicationListResult,
  ProductPublicationRow,
  ProductPublicationView,
  PublishRevisionCommand,
  PublishRevisionResult,
  ResolvePublishUnknownCommand,
  RetryDeliveryCommand,
  RetryDeliveryResult,
} from "@/features/product-publications/types"
import {
  DELIVERY_STATUS_LABEL,
  DELIVERY_STATUS_TONE,
  PUBLICATION_STATUS_LABEL,
  PUBLICATION_STATUS_TONE,
  SALE_STATUS_LABEL,
} from "@/features/product-publications/types"
import {
  CREATION_BLOCKER,
  DATA_WATERMARK,
  MALLS,
  PUBLICATION_SEEDS,
  type PublicationSeed,
} from "@/mock/product-publications"
import {
  getW22PublicationOverride,
  listW22SessionPublications,
  resolveW22PublishUnknown,
  submitW22ManualPause,
  submitW22PublishRevision,
  submitW22RetryDelivery,
} from "@/mock/product-publications-session"

const PERMISSION_VERSION = "pv-w22-demo-1"
const DATA_SCOPE_VERSION = "ds-w22-demo-1"

function allSeeds(): PublicationSeed[] {
  const session = listW22SessionPublications()
  const sessionIds = new Set(session.map((s) => s.row.publicationId))
  const base = PUBLICATION_SEEDS.filter(
    (s) => !sessionIds.has(s.row.publicationId)
  )
  const merged = [...session, ...base].map((seed) => {
    const override = getW22PublicationOverride(seed.row.publicationId)
    return override ?? seed
  })
  return merged
}

function isPendingConfirm(row: ProductPublicationRow): boolean {
  if (!row.latestDelivery) return false
  const s = row.latestDelivery.status
  return s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
}

function isFailedOrHandoff(row: ProductPublicationRow): boolean {
  const s = row.latestDelivery?.status
  return s === "FAILED" || s === "HANDOFF"
}

function isPaused(row: ProductPublicationRow): boolean {
  return (
    row.publicationStatus === "PAUSED" ||
    row.publicationStatus === "SAFETY_PAUSED"
  )
}

function computeMetrics(rows: readonly ProductPublicationRow[]) {
  return {
    pendingPublish: rows.filter((r) => r.publicationStatus === "PENDING_PUBLISH")
      .length,
    pendingConfirm: rows.filter(isPendingConfirm).length,
    failedOrHandoff: rows.filter(isFailedOrHandoff).length,
    mallLive: rows.filter((r) => r.publicationStatus === "MALL_LIVE").length,
    paused: rows.filter(isPaused).length,
  }
}

function matchSearch(q: string | undefined, parts: readonly string[]): boolean {
  if (!q?.trim()) return true
  const needle = q.trim().toLowerCase()
  return parts.some((p) => p.toLowerCase().includes(needle))
}

function filterSummary(
  query: ProductPublicationListQuery,
  total: number
): string {
  const parts: string[] = []
  if (query.metric && query.metric !== "all") {
    const labels: Record<string, string> = {
      pending_confirm: "待商城确认",
      failed_handoff: "失败/转人工",
      mall_live: "商城已生效",
      paused: "已暂停",
      pending_publish: "待发布",
    }
    parts.push(labels[query.metric] ?? query.metric)
  }
  if (query.deliveryStatus && query.deliveryStatus !== "all") {
    const labels: Record<string, string> = {
      pending_confirm: "待商城确认",
      failed: "失败",
      handoff: "转人工",
      acked: "已确认",
    }
    parts.push(labels[query.deliveryStatus] ?? query.deliveryStatus)
  }
  if (query.publicationStatus && query.publicationStatus !== "all") {
    parts.push(
      PUBLICATION_STATUS_LABEL[
        query.publicationStatus as keyof typeof PUBLICATION_STATUS_LABEL
      ] ?? query.publicationStatus
    )
  }
  if (query.mallId) {
    parts.push(MALLS.find((m) => m.id === query.mallId)?.name ?? query.mallId)
  }
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  parts.push(`${total} 条`)
  return parts.join(" · ")
}

export async function fetchPublicationList(
  query: ProductPublicationListQuery
): Promise<ProductPublicationListResult> {
  await mockDelay()

  const seeds = allSeeds()
  let rows = seeds.map((s) => s.row)

  // metrics 基于全量（当前数据范围）
  const metrics = computeMetrics(rows)

  if (query.q?.trim()) {
    rows = rows.filter((r) =>
      matchSearch(query.q, [
        r.publicationCode,
        r.skuCode,
        r.productName,
        r.targetMallName,
        r.publicationId,
      ])
    )
  }
  if (query.skuId) {
    rows = rows.filter((row) => row.skuId === query.skuId)
  }
  if (query.supplierOfferingRevisionId) {
    rows = rows.filter(
      (row) =>
        row.fixedOffering.offeringRevisionId === query.supplierOfferingRevisionId
    )
  }
  if (query.mallId) {
    rows = rows.filter((r) => r.targetMallId === query.mallId)
  }
  if (query.publicationStatus && query.publicationStatus !== "all") {
    rows = rows.filter((r) => r.publicationStatus === query.publicationStatus)
  }
  if (query.deliveryStatus && query.deliveryStatus !== "all") {
    if (query.deliveryStatus === "pending_confirm") {
      rows = rows.filter(isPendingConfirm)
    } else if (query.deliveryStatus === "failed") {
      rows = rows.filter((r) => r.latestDelivery?.status === "FAILED")
    } else if (query.deliveryStatus === "handoff") {
      rows = rows.filter((r) => r.latestDelivery?.status === "HANDOFF")
    } else if (query.deliveryStatus === "acked") {
      rows = rows.filter((r) => r.latestDelivery?.status === "ACKED")
    }
  }
  if (query.metric && query.metric !== "all") {
    if (query.metric === "pending_confirm") rows = rows.filter(isPendingConfirm)
    else if (query.metric === "failed_handoff")
      rows = rows.filter(isFailedOrHandoff)
    else if (query.metric === "mall_live")
      rows = rows.filter((r) => r.publicationStatus === "MALL_LIVE")
    else if (query.metric === "paused") rows = rows.filter(isPaused)
    else if (query.metric === "pending_publish")
      rows = rows.filter((r) => r.publicationStatus === "PENDING_PUBLISH")
  }

  // 默认排除失效
  if (!query.publicationStatus || query.publicationStatus === "all") {
    // keep invalid only when explicitly filtered; default shows non-invalid
    if (!query.metric || query.metric === "all") {
      rows = rows.filter((r) => r.publicationStatus !== "INVALID")
    }
  }

  rows = [...rows].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))

  const page = query.page ?? 1
  const pageSize = query.pageSize ?? 20
  const total = rows.length
  const start = (page - 1) * pageSize
  const items = rows.slice(start, start + pageSize)

  const hasFilters = Boolean(
    query.q?.trim() ||
      query.mallId ||
      (query.publicationStatus && query.publicationStatus !== "all") ||
      (query.deliveryStatus && query.deliveryStatus !== "all") ||
      (query.metric && query.metric !== "all")
  )

  let emptyReason: ProductPublicationListResult["emptyReason"]
  if (total === 0) {
    emptyReason = hasFilters ? "FILTER_NO_RESULT" : "NO_DATA"
  }

  return {
    items,
    total,
    page,
    pageSize,
    metrics,
    permissionVersion: PERMISSION_VERSION,
    dataScopeVersion: DATA_SCOPE_VERSION,
    queriedAt: new Date().toISOString(),
    creationBlocker: CREATION_BLOCKER,
    filterSummary: filterSummary(query, total),
    emptyReason,
  }
}

export async function fetchPublicationDetail(
  publicationId: string,
  revisionId?: string
): Promise<ProductPublicationView | null> {
  await mockDelay(60)
  const seed = allSeeds().find((s) => s.row.publicationId === publicationId)
  if (!seed) return null

  const selected =
    seed.revisions.find((r) => r.revisionId === revisionId) ??
    seed.revisions.find((r) => r.revisionId === seed.row.latestRevisionId) ??
    seed.revisions[seed.revisions.length - 1]

  if (!selected) return null

  const latest = seed.revisions[seed.revisions.length - 1]
  const ackedId = seed.row.currentAckedRevisionId

  return {
    identity: {
      publicationId: seed.row.publicationId,
      publicationCode: seed.row.publicationCode,
      skuId: seed.row.skuId,
      skuCode: seed.row.skuCode,
      targetMallId: seed.row.targetMallId,
      targetMallName: seed.row.targetMallName,
    },
    status: seed.row.publicationStatus,
    statusLabel: seed.row.publicationStatusLabel,
    statusTone: seed.row.publicationStatusTone,
    currentAckedRevisionId: seed.row.currentAckedRevisionId,
    currentAckedRevisionNo: seed.row.currentAckedRevisionNo,
    latestRevisionId: latest?.revisionId,
    latestRevisionNo: latest?.revisionNo,
    selectedRevision: selected,
    revisions: seed.revisions.map((r) => {
      const delivery = seed.deliveries.find((d) => d.revisionId === r.revisionId)
      return {
        revisionId: r.revisionId,
        revisionNo: r.revisionNo,
        saleStatus: r.saleStatus,
        saleStatusLabel: r.saleStatusLabel,
        createdAt: r.createdAt,
        createdBy: r.createdBy,
        contentHash: r.contentHash,
        deliverySummary: delivery
          ? `${delivery.statusLabel}${delivery.errorSummary ? ` · ${delivery.errorSummary}` : ""}`
          : "无发送",
        isMallAcked: r.revisionId === ackedId,
        isLatest: r.revisionId === latest?.revisionId,
      }
    }),
    deliveries: seed.deliveries,
    safetyPause: seed.row.safetyPause,
    publishGate: seed.publishGate,
    freshness: {
      queriedAt: new Date().toISOString(),
      integrationUpdatedAt: DATA_WATERMARK,
    },
    allowedActions: seed.allowedActions,
    actionBlockers: seed.actionBlockers,
    fieldPermissions: seed.fieldPermissions,
    objectVersion: seed.objectVersion,
    ownerLabel: seed.ownerLabel,
  }
}

export async function publishRevision(
  command: PublishRevisionCommand
): Promise<PublishRevisionResult> {
  await mockDelay(150)
  return submitW22PublishRevision(command)
}

export async function resolvePublishUnknown(
  command: ResolvePublishUnknownCommand
): Promise<PublishRevisionResult> {
  await mockDelay(80)
  return resolveW22PublishUnknown(command)
}

export async function manualPausePublication(
  command: ManualPauseCommand
): Promise<ManualPauseResult> {
  await mockDelay(120)
  return submitW22ManualPause(command)
}

export async function retryDelivery(
  command: RetryDeliveryCommand
): Promise<RetryDeliveryResult> {
  await mockDelay(100)
  return submitW22RetryDelivery(command)
}

export { PUBLICATION_STATUS_LABEL, PUBLICATION_STATUS_TONE }
export { DELIVERY_STATUS_LABEL, DELIVERY_STATUS_TONE, SALE_STATUS_LABEL, MALLS }
