import {
    ClipboardListIcon,
    FileCheck2Icon,
    ReceiptTextIcon,
    ShoppingCartIcon,
    TruckIcon,
    WalletCardsIcon,
} from "lucide-react"
import type { WorkspaceSearchOption } from "@/lib/workspace-search"

/** 候选保留服务端身份，只将显示名称收敛为用户可直接识别的事项类型。 */
export function queueTypeLabel(option: WorkspaceSearchOption): string {
    if (option.value === "task:DOCUMENT_APPROVAL") return "全部审批"
    if (option.label.startsWith("审批 · "))
        return `${option.label.slice(5)}审批`
    return option.label.replace(/^执行 · /, "")
}

/** 常驻入口数量固定；其余类型在同一区域展开显示。 */
export function queueQuickTypes(options: readonly WorkspaceSearchOption[]) {
    const preferred = [
        "approval:purchase_order",
        "approval:sales_order",
        "approval:purchase_payment_request",
        "task:PROCUREMENT_ORDER_CREATION",
        "task:FULFILLMENT_OPERATION",
    ]
    const rank = (value: string) => {
        const index = preferred.indexOf(value)
        return index < 0 ? preferred.length : index
    }
    return options
        .filter((option) => option.value !== "task:DOCUMENT_APPROVAL")
        .sort((left, right) => rank(left.value) - rank(right.value))
        .slice(0, 5)
}

/** 导航按钮与表格行复用同一语义图标，避免同一种事项出现不同视觉标识。 */
export function queueTypeIcon(value: string) {
    if (
        value.includes("payment") ||
        value.includes("PAYMENT") ||
        value.includes("refund") ||
        value.includes("REFUND")
    )
        return WalletCardsIcon
    if (value.includes("invoice") || value.includes("INVOICE"))
        return ReceiptTextIcon
    if (
        value.includes("FULFILLMENT") ||
        value.includes("delivery") ||
        value.includes("RECEIPT")
    )
        return TruckIcon
    if (value.includes("purchase") || value.includes("PROCUREMENT"))
        return ShoppingCartIcon
    if (value.startsWith("approval:")) return FileCheck2Icon
    return ClipboardListIcon
}
