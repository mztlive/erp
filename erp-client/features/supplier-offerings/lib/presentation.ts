import type { OfferingStatus } from "@/features/supplier-offerings/types"

/** 返回关系状态对应的徽标样式。 */
export const statusVariant = (status: OfferingStatus) => {
    if (status === "ACTIVE") return "success" as const
    if (status === "STOPPED") return "destructive" as const
    return "secondary" as const
}

/** 格式化可选金额。 */
export const money = (value?: string | null): string => {
    return value ? `¥${value}` : "—"
}
