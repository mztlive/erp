export type OrgUnitKind = "department" | "team"

export type OrgUnit = {
    id: string
    name: string
    parent_id: string | null
    kind: OrgUnitKind
    enabled: boolean
    version: number
    reason: string
}

export type OrgMembership = {
    id: string
    user_id: string
    org_unit_id: string
    valid_from: number
    valid_to: number | null
    reason: string
}

export type OrgManagementAssignment = {
    id: string
    user_id: string
    role_id: string
    org_unit_id: string
    include_descendants: boolean
    valid_from: number
    valid_to: number | null
    reason: string
}

export type OrgPerson = {
    id: string
    label: string
    account: string
    active: boolean
    own_org_unit_id: string | null
}

export type OrgRole = {
    id: string
    name: string
    enabled: boolean
}

export type OrganizationState = {
    version: number
    units: OrgUnit[]
    memberships: OrgMembership[]
    management: OrgManagementAssignment[]
}

export type OrganizationStateView = OrganizationState & {
    people: OrgPerson[]
    roles: OrgRole[]
    scopeVersion: string
    policyVersion: number
    organizationVersion: number
    asOf: string
    emptyReason: "no_scope" | null
    scopeSummary: string
    ownershipBasis: string
}

export type OrganizationOperation =
    | {
          operation: "create_unit"
          name: string
          parent_id: string | null
          kind: OrgUnitKind
      }
    | {
          operation: "move_unit"
          org_unit_id: string
          parent_id: string | null
      }
    | {
          operation: "rename_unit"
          org_unit_id: string
          name: string
      }
    | { operation: "disable_unit"; org_unit_id: string }
    | {
          operation: "transfer_member"
          user_id: string
          org_unit_id: string
      }
    | { operation: "end_membership"; user_id: string }
    | {
          operation: "grant_management"
          user_id: string
          role_id: string
          org_unit_id: string
          include_descendants: boolean
          valid_to: number | null
      }
    | { operation: "revoke_management"; assignment_id: string }

export type OrganizationChangeRequest = {
    expected_version: number
    idempotency_key: string
    reason: string
    change: OrganizationOperation
}

export type OrganizationChangeReceipt = {
    id: string
    actor_id: string
    request: OrganizationChangeRequest
    before: OrganizationState
    after: OrganizationState
    as_of: number
}

export type OrganizationEmptyReason =
    | "NO_MODULE_PERMISSION"
    | "NO_DATA_SCOPE"
    | "NO_RECORDS_IN_SCOPE"
    | "FILTER_NO_RESULT"

export type OrganizationUrlState = {
    unitId?: string
    q?: string
    kind: "all" | OrgUnitKind
    status: "all" | "enabled" | "disabled"
}

export type DataScopeSubjectType = "role" | "user"

export type DataScopeType =
    | "company"
    | "organization"
    | "team"
    | "self_owned"
    | "collaborative"

export type ScopeDimension = "internal_org" | "settlement_party" | "warehouse"

export type ScopeTargetMode = "explicit" | "own_org" | "managed_orgs"

export type DataScopeRecord = {
    id: string
    subjectType: DataScopeSubjectType
    subjectId: string
    scopeType: DataScopeType
    scopeTargets: string[]
    schemaVersion: number
    resource: string
    actions: string[]
    targetDimension: ScopeDimension
    targetMode: ScopeTargetMode | null
    includeDescendants: boolean | null
    enabled: boolean
    version: number
    createdAt: number
}

export type DataScopeListView = {
    items: DataScopeRecord[]
    total: number
    emptyReason: "no_scope" | null
    scopeVersion?: string
    policyVersion?: number
    organizationVersion?: number
}

export type DataScopeUrlState = {
    q?: string
    resource?: string
    action?: string
    subjectType: "all" | DataScopeSubjectType
    subjectId?: string
    scopeType: "all" | DataScopeType
}

export type CreateDataScopeInput = {
    subjectType: DataScopeSubjectType
    subjectId: string
    scopeType: DataScopeType
    resource: string
    actions: string[]
    targetDimension: ScopeDimension
    targetMode: ScopeTargetMode | null
    includeDescendants: boolean | null
    scopeTargets: string[]
}
