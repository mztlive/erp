/**
 * W22 商品发布 · 发布详情查询（真实 HTTP）。
 */

import { apiGet, type Page } from "@/lib/api"

import {
    emptyFixedOffering,
    mapDeliveryStatus,
    mapPublicationStatus,
    mapSaleStatus,
    secsToIso,
} from "@/features/product-publications/api/mappers"
import { loadMalls, mallName } from "@/features/product-publications/api/malls"
import type {
    BackendDelivery,
    BackendMedia,
    BackendPublication,
    BackendRevision,
} from "@/features/product-publications/api/wire-types"
import {
    DELIVERY_STATUS_LABEL,
    DELIVERY_STATUS_TONE,
    PUBLICATION_STATUS_LABEL,
    PUBLICATION_STATUS_TONE,
    SALE_STATUS_LABEL,
} from "@/features/product-publications/lib/status-labels"
import type { ProductPublicationView } from "@/features/product-publications/types"

export async function fetchPublicationDetail(
    publicationId: string,
    revisionId?: string,
): Promise<ProductPublicationView | null> {
    const malls = await loadMalls()

    let pub: BackendPublication
    try {
        pub = await apiGet<BackendPublication>(
            `/admin/product-publications/${encodeURIComponent(publicationId)}`,
        )
    } catch (err) {
        const e = err as { kind?: string; status?: number }
        if (e?.kind === "Http" && e.status === 404) return null
        throw err
    }

    const revisions = await apiGet<BackendRevision[]>(
        `/admin/product-publications/${encodeURIComponent(publicationId)}/revisions`,
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
        `/admin/product-publication-revisions/${encodeURIComponent(selected.id)}/media`,
    ).catch(() => [] as BackendMedia[])

    const deliveryPage = await apiGet<Page<BackendDelivery>>(
        "/admin/product-publication-deliveries",
        {
            page: 1,
            page_size: 100,
            target_mall_id: pub.target_mall_id,
        },
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
            const rev = revisions.find(
                (r) => r.id === d.publication_revision_id,
            )
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
