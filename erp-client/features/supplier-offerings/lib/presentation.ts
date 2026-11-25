import type { StatusTone } from "@/components/ui/status-badge"
import type { OfferingStatus } from "@/features/supplier-offerings/types"

/** 返回关系状态对应的语义色。 */
export const statusTone = (status: OfferingStatus): StatusTone => {
    if (status === "ACTIVE") return "success"
    if (status === "STOPPED") return "destructive"
    return "neutral"
}

/** 格式化可选金额。 */
export const money = (value?: string | null): string => {
    return value ? `¥${value}` : "—"
}
