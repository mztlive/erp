"use client"

import type { ReactNode } from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

/** 交互入口缺少任一所需权限时不挂载；业务状态限制仍由入口自身处理。 */
export function PermissionGuard({
    permission,
    children,
}: {
    permission: string | readonly string[]
    children: ReactNode
}) {
    const profile = useAccountProfileQuery()
    const required = typeof permission === "string" ? [permission] : permission
    if (
        !profile.isSuccess ||
        !required.every((code) => hasPermission(profile.data.permissions, code))
    )
        return null
    return children
}
