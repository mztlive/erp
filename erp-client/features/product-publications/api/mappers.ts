/**
 * W22 商品发布 · HTTP 适配层通用映射与格式化纯函数。
 */

import type {
    DeliveryStatus,
    PublicationStatus,
    SaleStatus,
} from "@/features/product-publications/types"

export function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return new Date(0).toISOString()
    return new Date(secs * 1000).toISOString()
}

export function mapPublicationStatus(raw: string): PublicationStatus {
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

export function toBackendPublicationStatus(s: string): string | undefined {
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

export function mapDeliveryStatus(raw: string): DeliveryStatus {
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

export function mapSaleStatus(raw: string): SaleStatus {
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

export function toBackendSaleStatus(s: SaleStatus): string {
    switch (s) {
        case "ON_SALE":
            return "on_sale"
        case "OFF_SALE":
            return "off_sale"
        case "PAUSED":
            return "pause_order"
    }
}

export function toBackendMediaRole(role: string): string {
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

export function emptyFixedOffering() {
    return {
        offeringRevisionId: "",
        supplierName: "—",
        availability: "UNKNOWN",
        availabilityLabel: "未返回",
        supplyPriceVisible: false as const,
    }
}
