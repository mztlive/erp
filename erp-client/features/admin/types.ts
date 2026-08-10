/**
 * 系统管理 · 账号管理 / 角色管理 领域类型。
 * 与后端 iam 域 DTO（services/src/iam/dto.rs、iam/account/dto.rs）一一对应。
 */

/** 角色项（GET /admin/roles、GET /admin/roles/assignable）。 */
export type AdminRole = {
    id: string
    name: string
    /** 权限字符串（resource:action），如 "admin:list"。 */
    permissions: string[]
    created_at: number
}

/** 创建管理员请求（POST /admin/admins）。 */
export type CreateAdminPayload = {
    account: string
    password: string
    name: string
    role_ids: string[]
}

/** 更新管理员请求（PUT /admin/admins/{id}）；未提供字段不修改。 */
export type UpdateAdminPayload = {
    name?: string
    password?: string
    role_ids?: string[]
}

/** 更新管理员角色请求（PUT /admin/admins/{id}/role）。 */
export type UpdateAdminRolePayload = {
    role_ids: string[]
}

/** 创建角色请求（POST /admin/roles）。 */
export type CreateRolePayload = {
    name: string
    permissions: string[]
}

/** 更新角色请求（PUT /admin/roles/{id}）；未提供字段不修改。 */
export type UpdateRolePayload = {
    name?: string
    permissions?: string[]
}
