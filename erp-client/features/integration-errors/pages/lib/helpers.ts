import type {
    InterfaceErrorClass,
    InterfaceErrorStatus,
} from "@/components/business"
import type {
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"

export function newKey(prefix: string) {
    return `${prefix}:${crypto.randomUUID()}`
}

export function mapPanelStatus(
    item: IntegrationResolutionItemView,
): InterfaceErrorStatus {
    if (item.status.code === "AUTO_RETRYING") return "auto-retrying"
    if (
        item.status.label.includes("人工") ||
        item.status.code === "MANUAL_REQUIRED"
    )
        return "manual-required"
    if (
        item.status.code === "COMPLETED" ||
        item.status.label.includes("已解决")
    )
        return "resolved"
    if (item.status.code === "CLOSED" || item.status.label.includes("关闭"))
        return "closed"
    return "pending"
}

export function isPanelErrorClass(
    c: IntegrationResolutionItemView["classification"]["errorClass"],
): c is InterfaceErrorClass {
    return c !== "reconciliation-difference"
}

export function formalStatus(
    s: IntegrationFormalResult["status"],
): "succeeded" | "blocked" | "rejected" | "unknown" | "processing" {
    if (s === "failed") return "rejected"
    return s
}
