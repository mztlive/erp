import type { StatusTone } from "@/components/ui/status-badge"
import type { AdminAccountStatus } from "../types"

export type AdminAccountStatusBadge = {
    label: string
    tone: StatusTone
}

/** 账号生命周期状态的统一展示映射。 */
export function adminAccountStatusBadge(
    status: AdminAccountStatus,
): AdminAccountStatusBadge {
    switch (status) {
        case "active":
            return { label: "启用", tone: "success" }
        case "suspended":
            return { label: "停用", tone: "neutral" }
        case "archived":
            return { label: "已归档", tone: "neutral" }
        default:
            return { label: "状态未知", tone: "neutral" }
    }
}
