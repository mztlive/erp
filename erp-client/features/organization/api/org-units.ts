import { apiGet, apiPost } from "@/lib/api"
import type {
    OrganizationChangeReceipt,
    OrganizationChangeRequest,
    OrganizationStateView,
    OrgManagementAssignment,
    OrgMembership,
    OrgPerson,
    OrgRole,
    OrgUnit,
} from "@/features/organization/types"

type BackendOrgUnit = {
    id: string
    name: string
    parent_id?: string | null
    kind: OrgUnit["kind"]
    enabled: boolean
    version: number
    reason: string
}

type BackendMembership = {
    id: string
    user_id: string
    org_unit_id: string
    valid_from: number
    valid_to?: number | null
    reason: string
}

type BackendManagement = {
    id: string
    user_id: string
    role_id: string
    org_unit_id: string
    include_descendants: boolean
    valid_from: number
    valid_to?: number | null
    reason: string
}

type BackendPerson = {
    id: string
    label: string
    account: string
    active: boolean
    own_org_unit_id?: string | null
}

type BackendRole = {
    id: string
    name: string
    enabled: boolean
}

type BackendState = {
    version: number
    units: BackendOrgUnit[]
    memberships: BackendMembership[]
    management: BackendManagement[]
}

type BackendStateView = BackendState & {
    people?: BackendPerson[]
    roles?: BackendRole[]
    scope_version: string
    policy_version: number
    organization_version: number
    as_of: string
    empty_reason?: string | null
    scope_summary: string
    ownership_basis: string
}

function mapUnit(row: BackendOrgUnit): OrgUnit {
    return {
        id: row.id,
        name: row.name,
        parent_id: row.parent_id ?? null,
        kind: row.kind,
        enabled: row.enabled,
        version: row.version,
        reason: row.reason,
    }
}

function mapMembership(row: BackendMembership): OrgMembership {
    return {
        id: row.id,
        user_id: row.user_id,
        org_unit_id: row.org_unit_id,
        valid_from: row.valid_from,
        valid_to: row.valid_to ?? null,
        reason: row.reason,
    }
}

function mapManagement(row: BackendManagement): OrgManagementAssignment {
    return {
        id: row.id,
        user_id: row.user_id,
        role_id: row.role_id,
        org_unit_id: row.org_unit_id,
        include_descendants: row.include_descendants,
        valid_from: row.valid_from,
        valid_to: row.valid_to ?? null,
        reason: row.reason,
    }
}

function mapPerson(row: BackendPerson): OrgPerson {
    return {
        id: row.id,
        label: row.label,
        account: row.account,
        active: row.active,
        own_org_unit_id: row.own_org_unit_id ?? null,
    }
}

function mapRole(row: BackendRole): OrgRole {
    return {
        id: row.id,
        name: row.name,
        enabled: row.enabled,
    }
}

function mapState(row: BackendState) {
    return {
        version: row.version,
        units: row.units.map(mapUnit),
        memberships: row.memberships.map(mapMembership),
        management: row.management.map(mapManagement),
    }
}

export async function fetchOrganizationState(): Promise<OrganizationStateView> {
    const page = await apiGet<BackendStateView>("/admin/org-units")
    return {
        ...mapState(page),
        people: (page.people ?? []).map(mapPerson),
        roles: (page.roles ?? []).map(mapRole),
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        asOf: page.as_of,
        emptyReason: page.empty_reason === "no_scope" ? "no_scope" : null,
        scopeSummary: page.scope_summary,
        ownershipBasis: page.ownership_basis,
    }
}

export async function previewOrganizationChange(
    request: OrganizationChangeRequest,
): Promise<OrganizationChangeReceipt> {
    const receipt = await apiPost<
        BackendStateView & {
            id: string
            actor_id: string
            request: OrganizationChangeRequest
            before: BackendState
            after: BackendState
            as_of: number
        }
    >("/admin/org-units/preview", request)
    return {
        id: receipt.id,
        actor_id: receipt.actor_id,
        request: receipt.request,
        before: mapState(receipt.before),
        after: mapState(receipt.after),
        as_of: receipt.as_of,
    }
}

export async function submitOrganizationChange(
    request: OrganizationChangeRequest,
): Promise<OrganizationChangeReceipt> {
    const receipt = await apiPost<
        BackendStateView & {
            id: string
            actor_id: string
            request: OrganizationChangeRequest
            before: BackendState
            after: BackendState
            as_of: number
        }
    >("/admin/org-units/change", request)
    return {
        id: receipt.id,
        actor_id: receipt.actor_id,
        request: receipt.request,
        before: mapState(receipt.before),
        after: mapState(receipt.after),
        as_of: receipt.as_of,
    }
}
