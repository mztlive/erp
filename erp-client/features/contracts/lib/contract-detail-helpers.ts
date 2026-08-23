import type { ContractCenterView } from "@/features/contracts/types"

export type ContractDetailSectionId =
    | "overview"
    | "settlement"
    | "attachments"
    | "sales-orders"
    | "versions"

export const CONTRACT_SECTION_NAV: readonly {
    id: ContractDetailSectionId
    label: string
}[] = [
    { id: "overview", label: "概览" },
    { id: "settlement", label: "结算与开票" },
    { id: "attachments", label: "附件" },
    { id: "sales-orders", label: "关联销售单" },
    { id: "versions", label: "版本与审计" },
]

export function resolveSection(section?: string): ContractDetailSectionId {
    const found = CONTRACT_SECTION_NAV.find((item) => item.id === section)
    return found?.id ?? "overview"
}

export function contractSectionHref(
    contractId: string,
    section: ContractDetailSectionId,
): string {
    const base = `/sales/contracts/${contractId}`
    return section === "overview" ? base : `${base}?section=${section}`
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
