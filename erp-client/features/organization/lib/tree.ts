import type {
    OrganizationStateView,
    OrganizationUrlState,
    OrgManagementAssignment,
    OrgMembership,
    OrgUnit,
} from "@/features/organization/types"

export type OrgTreeNode = {
    unit: OrgUnit
    children: OrgTreeNode[]
    members: OrgMembership[]
    management: OrgManagementAssignment[]
}

export function personLabel(
    people: OrganizationStateView["people"],
    userId: string,
): string {
    const person = people.find((item) => item.id === userId)
    if (!person) return "人员信息待确认"
    return person.active
        ? `${person.label}（${person.account}）`
        : `${person.label}（${person.account}） · 已停用`
}

export function roleLabel(
    roles: OrganizationStateView["roles"],
    roleId: string,
): string {
    return roles.find((item) => item.id === roleId)?.name ?? "角色信息待确认"
}

export function unitLabel(units: OrgUnit[], id: string): string {
    return units.find((item) => item.id === id)?.name ?? "组织信息待确认"
}

export function matchesOrganizationFilters(
    unit: OrgUnit,
    url: OrganizationUrlState,
): boolean {
    if (url.kind !== "all" && unit.kind !== url.kind) return false
    if (url.status === "enabled" && !unit.enabled) return false
    if (url.status === "disabled" && unit.enabled) return false
    const keyword = url.q?.trim().toLowerCase()
    if (keyword && !unit.name.toLowerCase().includes(keyword)) return false
    return true
}

export function buildOrganizationForest(
    view: OrganizationStateView,
    url: OrganizationUrlState,
): OrgTreeNode[] {
    const visible = new Set(
        view.units
            .filter((unit) => matchesOrganizationFilters(unit, url))
            .map((unit) => unit.id),
    )
    const nodes = new Map<string, OrgTreeNode>(
        view.units.map((unit) => [
            unit.id,
            {
                unit,
                children: [],
                members: view.memberships.filter(
                    (item) => item.org_unit_id === unit.id,
                ),
                management: view.management.filter(
                    (item) => item.org_unit_id === unit.id,
                ),
            },
        ]),
    )
    const roots: OrgTreeNode[] = []
    for (const node of nodes.values()) {
        const parentId = node.unit.parent_id
        const parent = parentId ? nodes.get(parentId) : undefined
        if (parent) parent.children.push(node)
        else roots.push(node)
    }
    const prune = (node: OrgTreeNode): OrgTreeNode | null => {
        const children = node.children
            .map(prune)
            .filter((item): item is OrgTreeNode => item != null)
        if (!visible.has(node.unit.id) && children.length === 0) return null
        return { ...node, children }
    }
    return roots.map(prune).filter((item): item is OrgTreeNode => item != null)
}

export function flattenTree(nodes: OrgTreeNode[]): OrgTreeNode[] {
    return nodes.flatMap((node) => [node, ...flattenTree(node.children)])
}
