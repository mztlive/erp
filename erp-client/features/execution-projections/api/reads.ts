/**
 * W23 销售单执行投影 · 读路径 HTTP 适配（列表 / 详情 / 协同摘要）。
 * 调用点必须是 TanStack Query 的 queryFn（AGENTS.md 第 2 节）。
 */

import { apiGet, type Page } from "@/lib/api"
import type {
    ExecutionProjectionListQuery,
    ExecutionProjectionListResult,
    ExecutionProjectionRow,
    ExecutionProjectionView,
    SalesOrderCollaborationSummary,
} from "@/features/execution-projections/types"
import {
    DELIVERY_STATUS_LABEL,
    DELIVERY_STATUS_TONE,
} from "@/features/execution-projections/types"
import {
    type BackendDelivery,
    type BackendProjection,
    type BackendRevision,
    computeMetrics,
    filterSummary,
    loadMalls,
    mallName,
    mapDeliveryStatus,
    mapSource,
    recomputeActions,
    secsToIso,
    toRow,
    whitelistFromRevision,
} from "@/features/execution-projections/api/mapping"

export async function fetchExecutionProjectionList(
    query: ExecutionProjectionListQuery = {},
): Promise<ExecutionProjectionListResult> {
    const malls = await loadMalls()
    const page = Math.max(1, query.page ?? 1)
    const pageSize = Math.min(50, Math.max(1, query.pageSize ?? 20))

    const listQuery: Record<string, unknown> = {
        page,
        page_size: pageSize,
        sort_by: "updated_at",
        sort_dir: "desc",
    }
    if (query.mallId) listQuery.target_mall_id = query.mallId

    const pageResult = await apiGet<Page<BackendProjection>>(
        "/admin/sales-order-projections",
        listQuery,
    )

    const rows: ExecutionProjectionRow[] = pageResult.items.map((projection) =>
        toRow(
            projection,
            projection.latest_revision ?? undefined,
            projection.latest_delivery ?? undefined,
            malls,
        ),
    )

    let filtered = rows
    if (query.q?.trim()) {
        const q = query.q.trim().toUpperCase()
        filtered = filtered.filter(
            (r) =>
                r.projectionNo.toUpperCase().includes(q) ||
                r.salesOrderNo.toUpperCase().includes(q) ||
                r.customerLabel.toUpperCase().includes(q) ||
                r.targetMallName.toUpperCase().includes(q),
        )
    }
    if (query.deliveryStatus) {
        const statuses = query.deliveryStatus
            .split(",")
            .map((s) => s.trim().toUpperCase())
        filtered = filtered.filter((r) => statuses.includes(r.delivery.status))
    }
    if (query.source && query.source !== "all") {
        filtered = filtered.filter((r) => r.projectionSource === query.source)
    }
    if (query.latency && query.latency !== "all") {
        filtered = filtered.filter((r) => r.latencyBand === query.latency)
    }
    if (query.reconciliation && query.reconciliation !== "all") {
        filtered = filtered.filter(
            (r) => r.reconciliationStatus === query.reconciliation,
        )
    }
    if (query.metric && query.metric !== "all") {
        if (query.metric === "pending_send") {
            filtered = filtered.filter((r) => r.delivery.status === "PENDING")
        } else if (query.metric === "inflight") {
            filtered = filtered.filter((r) =>
                ["SENDING", "RETRYING", "UNKNOWN"].includes(r.delivery.status),
            )
        } else if (query.metric === "timeout") {
            filtered = filtered.filter((r) => r.latencyBand === "over_sla")
        } else if (query.metric === "fail_manual") {
            filtered = filtered.filter((r) =>
                ["FAILED", "ESCALATED_MANUAL"].includes(r.delivery.status),
            )
        } else if (query.metric === "acked") {
            filtered = filtered.filter((r) => r.delivery.status === "ACKED")
        }
    }

    const asOf = secsToIso(
        Math.max(0, ...pageResult.items.map((p) => p.created_at)),
    )

    return {
        rows: filtered,
        pageInfo: {
            page: pageResult.page,
            pageSize: pageResult.page_size,
            total: pageResult.total,
        },
        metrics: computeMetrics(filtered),
        malls,
        permissionVersion: "pv-live",
        sourceFactsAsOf: asOf,
        projectionUpdatedAt: asOf,
        deliveryStatusUpdatedAt: asOf,
        queriedAt: asOf,
        filterSummary: filterSummary(query),
        defaultViewNote: "运营默认关注未确认与失败；结果未知不计入已确认指标。",
    }
}

export async function fetchExecutionProjectionDetail(input: {
    projectionId: string
    revisionId?: string
}): Promise<ExecutionProjectionView | null> {
    const malls = await loadMalls()

    let proj: BackendProjection
    try {
        proj = await apiGet<BackendProjection>(
            `/admin/sales-order-projections/${encodeURIComponent(input.projectionId)}`,
        )
    } catch (err) {
        const e = err as { kind?: string; status?: number }
        if (e?.kind === "Http" && e.status === 404) return null
        throw err
    }

    const revisions = await apiGet<BackendRevision[]>(
        `/admin/sales-order-projections/${encodeURIComponent(input.projectionId)}/revisions`,
    ).catch(() => [] as BackendRevision[])

    const selected =
        revisions.find((r) => r.id === input.revisionId) ?? revisions[0]
    const deliveryPage = await apiGet<Page<BackendDelivery>>(
        "/admin/sales-order-projection-deliveries",
        {
            page: 1,
            page_size: 100,
            target_mall_id: proj.target_mall_id,
        },
    ).catch(() => ({
        items: [] as BackendDelivery[],
        total: 0,
        page: 1,
        page_size: 100,
    }))

    const revIds = new Set(revisions.map((r) => r.id))
    const deliveries = deliveryPage.items
        .filter((d) => revIds.has(d.projection_revision_id))
        .map((d) => {
            const st = mapDeliveryStatus(d.status)
            return {
                deliveryId: d.id,
                status: st,
                statusLabel: DELIVERY_STATUS_LABEL[st],
                statusTone: DELIVERY_STATUS_TONE[st],
                attemptCount: d.attempt_count,
                lastAttemptAt: secsToIso(d.last_attempt_at ?? d.created_at),
                nextAttemptAt: d.next_attempt_at
                    ? secsToIso(d.next_attempt_at)
                    : undefined,
                mallAckAt: d.mall_ack_at ? secsToIso(d.mall_ack_at) : undefined,
                mallExecutionBaseline: d.mall_execution_baseline ?? undefined,
                errorCode: d.error_code ?? undefined,
                errorSummary: d.error_summary ?? undefined,
                errorTaskId: d.error_task_id ?? undefined,
                workItemId: d.work_item_id ?? undefined,
            }
        })

    const selectedDelivery = deliveryPage.items.find(
        (delivery) => delivery.projection_revision_id === selected?.id,
    )
    const primaryDelivery = deliveries.find(
        (delivery) => delivery.deliveryId === selectedDelivery?.id,
    )
    const orderedDeliveries = primaryDelivery
        ? [
              primaryDelivery,
              ...deliveries.filter(
                  (delivery) =>
                      delivery.deliveryId !== primaryDelivery.deliveryId,
              ),
          ]
        : deliveries
    const status = primaryDelivery?.status ?? "PENDING"
    const actions = selectedDelivery?.allowed_actions
        ? {
              allowedActions: selectedDelivery.allowed_actions,
              actionBlockers: selectedDelivery.action_blockers ?? [],
          }
        : recomputeActions(status)
    const content = selected
        ? whitelistFromRevision(selected)
        : {
              customerExternalIdentity: "—",
              customerExternalIdentityCopyable: false,
              voucherCategoryExternalIdentity: "—",
              voucherCategoryErpName: "—",
              voucherExpiryAt: "—",
              faceValue: "0",
              cardCount: "0",
              cardForm: "—",
              effectiveAt: secsToIso(proj.created_at),
              contentHash: proj.id,
          }

    const asOf = secsToIso(proj.created_at)

    return {
        identity: {
            projectionId: proj.id,
            projectionNo: proj.id.slice(0, 12).toUpperCase(),
            salesOrderId: proj.sales_order_id,
            salesOrderNo: proj.sales_order_id,
            targetMallId: proj.target_mall_id,
            targetMallName: mallName(malls, proj.target_mall_id),
        },
        tracks: {
            salesFact: {
                label: "销售事实",
                tone: "info",
                description: selected
                    ? `ERP 销售版本 ${selected.sales_order_revision_id} 已形成；接收失败不回退销售记录或应收。`
                    : "尚无投影修订。",
            },
            projectionDelivery: {
                label: DELIVERY_STATUS_LABEL[status],
                tone: DELIVERY_STATUS_TONE[status],
                description:
                    status === "ACKED"
                        ? "信息发送已完成"
                        : `尝试 ${primaryDelivery?.attemptCount ?? 0} 次`,
            },
            mallConfirm: {
                label: status === "ACKED" ? "已确认" : "尚未确认",
                tone: status === "ACKED" ? "success" : "neutral",
                description: primaryDelivery?.mallAckAt
                    ? `商城确认时间 ${primaryDelivery.mallAckAt}`
                    : "尚无明确商城确认时间",
            },
        },
        selectedRevision: {
            projectionRevisionId: selected?.id ?? "",
            revisionNo: selected?.revision_no ?? 0,
            projectionSource: selected
                ? mapSource(selected.projection_source)
                : "ERP_SALES_REVISION",
            salesOrderRevisionId: selected?.sales_order_revision_id ?? "",
            salesOrderRevisionNo: selected?.revision_no ?? 0,
            content,
        },
        revisionLinks: revisions.map((r) => {
            const d = deliveries.find((x) => x.deliveryId && x)
            const ds =
                deliveryPage.items.find(
                    (x) => x.projection_revision_id === r.id,
                )?.status ?? "pending_send"
            const st = mapDeliveryStatus(ds)
            return {
                salesOrderRevisionId: r.sales_order_revision_id,
                salesOrderRevisionNo: r.revision_no,
                projectionRevisionId: r.id,
                projectionRevisionNo: r.revision_no,
                deliveryStatus: st,
                deliveryStatusLabel: DELIVERY_STATUS_LABEL[st],
                mallAckAt: d?.mallAckAt,
                sourceSalesRevisionNo: r.revision_no,
                isCurrentSelection: r.id === (selected?.id ?? ""),
            }
        }),
        deliveries: orderedDeliveries,
        salesOrderStatus: "—",
        salesOrderStatusTone: "neutral",
        ownerLabel: "—",
        pendingDurationLabel: "—",
        latencyBand: "normal",
        reconciliationStatus: "NONE",
        allowedActions: actions.allowedActions,
        actionBlockers: actions.actionBlockers,
        fieldPermissions: {
            customerExternalIdentity: "masked",
            faceValue: "full",
            cardCount: "full",
            cardForm: "full",
            voucherExpiryAt: "full",
            contentHash: "full",
        },
        objectVersion: String(selectedDelivery?.version ?? proj.version),
        sourceFactsAsOf: asOf,
        projectionUpdatedAt: asOf,
        deliveryStatusUpdatedAt: asOf,
        queriedAt: asOf,
        boundaryNotice:
            "数据不是销售单副本。接收失败不回退销售记录、销售版本或应收；业务内容变更须在销售单走变更单形成新版本后自动产生新数据。",
    }
}

export async function fetchSalesOrderCollaboration(
    salesOrderId: string,
): Promise<SalesOrderCollaborationSummary> {
    const page = await apiGet<Page<BackendProjection>>(
        "/admin/sales-order-projections",
        {
            sales_order_id: salesOrderId,
            page: 1,
            page_size: 10,
        },
    ).catch(() => ({
        items: [] as BackendProjection[],
        total: 0,
        page: 1,
        page_size: 10,
    }))

    if (page.items.length === 0) {
        return {
            salesOrderId,
            salesOrderNo: salesOrderId,
            hasProjection: false,
            historyCount: 0,
            note: "当前销售单尚无执行信息。卡券销售版本生效后由系统自动形成数据。",
        }
    }

    const proj = page.items[0]!
    const detail = await fetchExecutionProjectionDetail({
        projectionId: proj.id,
    })
    if (!detail) {
        return {
            salesOrderId,
            salesOrderNo: salesOrderId,
            hasProjection: true,
            projectionId: proj.id,
            projectionNo: proj.id.slice(0, 12).toUpperCase(),
            historyCount: 0,
            w23Href: `/commerce/execution-projections?projectionId=${encodeURIComponent(proj.id)}`,
            note: "已有投影身份，详情加载失败。",
        }
    }

    return {
        salesOrderId: detail.identity.salesOrderId,
        salesOrderNo: detail.identity.salesOrderNo,
        hasProjection: true,
        projectionId: detail.identity.projectionId,
        projectionNo: detail.identity.projectionNo,
        salesOrderRevisionNo: detail.selectedRevision.salesOrderRevisionNo,
        projectionRevisionNo: detail.selectedRevision.revisionNo,
        targetMallName: detail.identity.targetMallName,
        tracks: detail.tracks,
        delivery: detail.deliveries[0],
        whitelistPreview: {
            voucherCategoryErpName:
                detail.selectedRevision.content.voucherCategoryErpName,
            faceValue: detail.selectedRevision.content.faceValue,
            cardCount: detail.selectedRevision.content.cardCount,
            cardForm: detail.selectedRevision.content.cardForm,
            voucherExpiryAt: detail.selectedRevision.content.voucherExpiryAt,
        },
        currentAckedRevisionNo: detail.currentAckedRevisionNo,
        reconciliationStatus: detail.reconciliationStatus,
        historyCount: detail.revisionLinks.length,
        w23Href: `/commerce/execution-projections?projectionId=${encodeURIComponent(proj.id)}`,
        historyHref: `/commerce/execution-projections?projectionId=${encodeURIComponent(proj.id)}`,
        note: "投影字段仅含商城执行白名单，不含成交金额/配赠/税率/开票/应收。",
    }
}
