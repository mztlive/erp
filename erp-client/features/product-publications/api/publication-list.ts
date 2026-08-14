/**
 * W22 商品发布 · 发布列表查询（真实 HTTP + 客户端补筛/指标）。
 */

import { apiGet, type Page } from "@/lib/api"

import {
    emptyFixedOffering,
    mapDeliveryStatus,
    mapPublicationStatus,
    secsToIso,
    toBackendPublicationStatus,
} from "@/features/product-publications/api/mappers"
import { loadMalls, mallName } from "@/features/product-publications/api/malls"
import type {
    BackendDelivery,
    BackendPublication,
    BackendRevision,
} from "@/features/product-publications/api/wire-types"
import {
    DELIVERY_STATUS_LABEL,
    DELIVERY_STATUS_TONE,
    PUBLICATION_STATUS_LABEL,
    PUBLICATION_STATUS_TONE,
} from "@/features/product-publications/lib/status-labels"
import type {
    ProductPublicationListQuery,
    ProductPublicationListResult,
    ProductPublicationRow,
} from "@/features/product-publications/types"

function rowFromPublication(
    pub: BackendPublication,
    rev: BackendRevision | undefined,
    delivery: BackendDelivery | undefined,
    malls: Array<{ id: string; name: string }>,
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
                rev.id !== pub.current_revision_id,
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
        allowedActions: [
            "OPEN_CENTER",
            "PUBLISH_REVISION",
            "MANUAL_PAUSE",
            "RETRY_DELIVERY",
        ],
        actionBlockers: [],
    }
}

export async function fetchPublicationList(
    query: ProductPublicationListQuery,
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
        listQuery,
    )

    // 投递列表（用于 latest delivery 投影）
    const deliveryPage = await apiGet<Page<BackendDelivery>>(
        "/admin/product-publication-deliveries",
        { page: 1, page_size: 100 },
    ).catch(() => ({
        items: [] as BackendDelivery[],
        total: 0,
        page: 1,
        page_size: 100,
    }))

    const rows: ProductPublicationRow[] = []
    for (const pub of pageResult.items) {
        const revisions = await apiGet<BackendRevision[]>(
            `/admin/product-publications/${encodeURIComponent(pub.id)}/revisions`,
        ).catch(() => [] as BackendRevision[])
        const latest = revisions[0]
        const delivery = deliveryPage.items.find(
            (d) =>
                d.publication_revision_id === latest?.id ||
                d.target_mall_id === pub.target_mall_id,
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
                r.publicationId.toUpperCase().includes(q),
        )
    }
    if (query.deliveryStatus && query.deliveryStatus !== "all") {
        if (query.deliveryStatus === "pending_confirm") {
            filtered = filtered.filter((r) => {
                const s = r.latestDelivery?.status
                return (
                    s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
                )
            })
        } else if (query.deliveryStatus === "failed") {
            filtered = filtered.filter(
                (r) => r.latestDelivery?.status === "FAILED",
            )
        } else if (query.deliveryStatus === "handoff") {
            filtered = filtered.filter(
                (r) => r.latestDelivery?.status === "HANDOFF",
            )
        } else if (query.deliveryStatus === "acked") {
            filtered = filtered.filter(
                (r) => r.latestDelivery?.status === "ACKED",
            )
        }
    }
    if (query.metric && query.metric !== "all") {
        if (query.metric === "pending_confirm") {
            filtered = filtered.filter((r) => {
                const s = r.latestDelivery?.status
                return (
                    s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
                )
            })
        } else if (query.metric === "failed_handoff") {
            filtered = filtered.filter(
                (r) =>
                    r.latestDelivery?.status === "FAILED" ||
                    r.latestDelivery?.status === "HANDOFF",
            )
        } else if (query.metric === "mall_live") {
            filtered = filtered.filter(
                (r) => r.publicationStatus === "MALL_LIVE",
            )
        } else if (query.metric === "paused") {
            filtered = filtered.filter(
                (r) =>
                    r.publicationStatus === "PAUSED" ||
                    r.publicationStatus === "SAFETY_PAUSED",
            )
        } else if (query.metric === "pending_publish") {
            filtered = filtered.filter(
                (r) => r.publicationStatus === "PENDING_PUBLISH",
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
            (r) => r.publicationStatus === "PENDING_PUBLISH",
        ).length,
        pendingConfirm: filtered.filter((r) => {
            const s = r.latestDelivery?.status
            return s === "PENDING_SEND" || s === "SENDING" || s === "RETRYING"
        }).length,
        failedOrHandoff: filtered.filter(
            (r) =>
                r.latestDelivery?.status === "FAILED" ||
                r.latestDelivery?.status === "HANDOFF",
        ).length,
        mallLive: filtered.filter((r) => r.publicationStatus === "MALL_LIVE")
            .length,
        paused: filtered.filter(
            (r) =>
                r.publicationStatus === "PAUSED" ||
                r.publicationStatus === "SAFETY_PAUSED",
        ).length,
    }

    const hasFilters = Boolean(
        query.q?.trim() ||
            query.mallId ||
            query.skuId ||
            query.supplierOfferingRevisionId ||
            (query.publicationStatus && query.publicationStatus !== "all") ||
            (query.deliveryStatus && query.deliveryStatus !== "all") ||
            (query.metric && query.metric !== "all"),
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
            Math.max(0, ...pageResult.items.map((p) => p.created_at)),
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
