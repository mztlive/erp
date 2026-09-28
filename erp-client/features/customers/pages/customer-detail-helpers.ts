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
