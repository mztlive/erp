"use client"

/** 待调整角色的账号。 */
export type RoleAssignmentTarget = {
    userId: string
    displayName: string
    accountName: string
    roleIds: readonly string[]
}
