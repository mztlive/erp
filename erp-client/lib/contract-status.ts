import type { StatusTone } from "@/components/ui/status-badge"

/** 合同主状态：由服务端返回，前端不得推导。 */
export type ContractStatus = "EFFECTIVE" | "TERMINATED" | "EXPIRED"

export const CONTRACT_STATUS_LABEL: Record<ContractStatus, string> = {
    EFFECTIVE: "生效",
    TERMINATED: "终止",
    EXPIRED: "到期",
}

export const CONTRACT_STATUS_TONE: Record<ContractStatus, StatusTone> = {
    EFFECTIVE: "success",
    TERMINATED: "neutral",
    EXPIRED: "warning",
}
