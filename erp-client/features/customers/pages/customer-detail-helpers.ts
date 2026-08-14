import type {
    CustomerCenterView,
    CustomerSectionId,
} from "@/features/customers/types"

export const SECTION_NAV: readonly {
    id: CustomerSectionId
    label: string
}[] = [
    { id: "overview", label: "概览" },
    { id: "related", label: "合同与销售" },
    { id: "settlement", label: "票款摘要" },
    { id: "quality", label: "经营摘要" },
    { id: "audit", label: "归属与审计" },
]

export function resolveSection(section?: string | null): CustomerSectionId {
    const found = SECTION_NAV.find((item) => item.id === section)
    return found?.id ?? "overview"
}

export function can(customer: CustomerCenterView, action: string): boolean {
    return customer.allowedActions.includes(action)
}

export function blocker(
    customer: CustomerCenterView,
    action: string,
): string | undefined {
    return customer.actionBlockers.find((b) => b.action === action)?.message
}

export function ownerLabel(customer: CustomerCenterView): string {
    const owner = customer.assignments.find(
        (a) => a.role === "OWNER" && a.isCurrent,
    )
    return owner?.userName ?? "—"
}

export function collaboratorCount(customer: CustomerCenterView): number {
    return customer.assignments.filter(
        (a) => a.role === "COLLABORATOR" && a.isCurrent,
    ).length
}

export function collaboratorSummary(customer: CustomerCenterView): string {
    const cols = customer.assignments.filter(
        (a) => a.role === "COLLABORATOR" && a.isCurrent,
    )
    if (cols.length === 0) return "无有效协作"
    return cols
        .map((c) => {
            const period = c.effectiveTo
                ? `${c.effectiveFrom} ~ ${c.effectiveTo}`
                : `${c.effectiveFrom} 起`
            return `${c.userName}（${period}）`
        })
        .join("；")
}

export function collaboratorShortNames(customer: CustomerCenterView): string {
    const cols = customer.assignments.filter(
        (a) => a.role === "COLLABORATOR" && a.isCurrent,
    )
    if (cols.length === 0) return "无"
    return cols.map((c) => c.userName).join("、")
}
