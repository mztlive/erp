import type {
    CustomerAssignmentView,
    CustomerCenterView,
    CustomerSectionId,
} from "@/features/customers/types"

export type CustomerActiveSectionId =
    | "overview"
    | "related"
    | "settlement"
    | "audit"

export const SECTION_NAV: readonly {
    id: CustomerActiveSectionId
    label: string
}[] = [
    { id: "overview", label: "客户概览" },
    { id: "related", label: "合同与销售" },
    { id: "settlement", label: "票款与经营" },
    { id: "audit", label: "归属与记录" },
]

export function resolveSection(
    section?: string | null,
): CustomerActiveSectionId {
    if (section === "quality") return "settlement"
    if (section === "contacts") return "overview"
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

export function currentOwners(
    customer: CustomerCenterView,
): readonly CustomerAssignmentView[] {
    return customer.assignments.filter(
        (assignment) => assignment.role === "OWNER" && assignment.isCurrent,
    )
}

export function currentCollaborators(
    customer: CustomerCenterView,
): readonly CustomerAssignmentView[] {
    return customer.assignments.filter(
        (assignment) =>
            assignment.role === "COLLABORATOR" && assignment.isCurrent,
    )
}

export function historicalAssignments(
    customer: CustomerCenterView,
): readonly CustomerAssignmentView[] {
    return customer.assignments.filter((assignment) => !assignment.isCurrent)
}

export function ownerLabel(customer: CustomerCenterView): string {
    return (
        currentOwners(customer)
            .map((assignment) => assignment.userName)
            .join("、") || "—"
    )
}

export function customerMonogram(customer: CustomerCenterView): string {
    const source = (
        customer.currentRevision.shortName ||
        customer.currentRevision.legalName ||
        customer.customerNo
    ).trim()
    return source.slice(0, 1) || "客"
}

export type { CustomerSectionId }
