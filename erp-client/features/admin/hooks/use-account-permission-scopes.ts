"use client"

import { useQuery } from "@tanstack/react-query"
import { fetchAccountPermissionScopes } from "../api/account-permission-scopes"
import type { AdminAccount } from "../types"

/** 只在账号权限预览打开时读取数据范围，账号与角色集合共同确定缓存。 */
export function useAccountPermissionScopes(
    account: AdminAccount | null,
    open: boolean,
) {
    const roleIds = [...(account?.role_ids ?? [])].sort()
    return useQuery({
        queryKey: ["admin", "permission-scopes", account?.id, roleIds],
        queryFn: () => fetchAccountPermissionScopes(account!.id, roleIds),
        enabled: open && Boolean(account),
        refetchOnMount: "always",
        staleTime: 0,
    })
}
