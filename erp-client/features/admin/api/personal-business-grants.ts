import { apiGet, apiPost } from "@/lib/api"

export type PersonalBusinessGrant = {
    id: string
    version: number
    user_id: string
    role_id: string
    resource: string
    actions: string[]
    active_actions: string[]
    org_unit_ids: string[]
    include_descendants: boolean
}
export type GrantRoleOption = {
    id: string
    name: string
    resources: { resource: string; actions: string[] }[]
}
export type PersonalGrantList = {
    items: PersonalBusinessGrant[]
    roles: GrantRoleOption[]
    policy_version: number
}
export type PersonalGrantInput = {
    role_id: string
    resource: string
    actions: string[]
    org_unit_ids: string[]
    include_descendants: boolean
}
const path = (userId: string) =>
    `/admin/personal-business-grants/${encodeURIComponent(userId)}`
export const fetchPersonalGrants = (userId: string) =>
    apiGet<PersonalGrantList>(path(userId))
export const createPersonalGrant = (
    userId: string,
    grant: PersonalGrantInput,
    version: number,
) =>
    apiPost<Omit<PersonalBusinessGrant, "active_actions">>(path(userId), {
        grant,
        expected_policy_version: version,
    })
export const revokePersonalGrant = (
    userId: string,
    grant: PersonalBusinessGrant,
    version: number,
) =>
    apiPost<void>(`${path(userId)}/${encodeURIComponent(grant.id)}/revoke`, {
        version: grant.version,
        expected_policy_version: version,
    })
