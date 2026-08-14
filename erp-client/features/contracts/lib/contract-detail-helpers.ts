import type { ContractCenterView } from "@/features/contracts/types"

export type ContractDetailSectionId =
    | "overview"
    | "settlement"
    | "attachments"
    | "sales-orders"
    | "versions"

export function resolveSection(section?: string): ContractDetailSectionId {
    if (
        section === "settlement" ||
        section === "attachments" ||
        section === "sales-orders" ||
        section === "versions"
    ) {
        return section
    }
    return "overview"
}

/** 30 日内将到期：与列表页同口径（仍生效 + 有效期止在 30 天内）。 */
export function isExpiringWithin30Days(contract: ContractCenterView): boolean {
    if (contract.status !== "EFFECTIVE") return false
    const validTo = new Date(contract.currentRevision.validTo + "T00:00:00")
    if (Number.isNaN(validTo.getTime())) return false
    const now = new Date()
    const dayMs = 24 * 60 * 60 * 1000
    const diff = Math.ceil((validTo.getTime() - now.getTime()) / dayMs)
    return diff >= 0 && diff <= 30
}
