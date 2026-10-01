import { apiGet, apiPost } from "@/lib/api"

export type BuiltinRoleState = "missing" | "existing" | "disabled" | "deleted"

export type BuiltinRoleTemplate = {
    id: string
    name: string
    description: string
    permissions: string[]
    setup_requirements: string[]
    recommended: boolean
    state: BuiltinRoleState
    existing_name: string | null
    can_generate: boolean
}

export type BuiltinRoleCatalog = {
    policy_version: number
    templates: BuiltinRoleTemplate[]
}

export type GeneratedBuiltinRole = {
    id: string
    name: string
    created: boolean
    state: BuiltinRoleState
}

export const fetchRoleTemplates = (): Promise<BuiltinRoleCatalog> =>
    apiGet("/admin/role-templates")

export const generateBuiltinRoles = (payload: {
    template_ids: string[]
    expected_policy_version: number
}): Promise<GeneratedBuiltinRole[]> =>
    apiPost("/admin/role-templates/generate", payload)
