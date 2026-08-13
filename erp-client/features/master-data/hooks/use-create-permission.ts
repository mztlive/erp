"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

/** 列表「新建」按钮的权限与阻断文案。 */
export function useCreatePermission(permission: string | undefined) {
    const accountQuery = useAccountProfileQuery()
    const canCreate = permission
        ? hasPermission(accountQuery.data?.permissions, permission)
        : false
    const createBlockedReason = accountQuery.isPending
        ? "正在核对创建权限，请稍候。"
        : accountQuery.isError
          ? getErrorMessage(
                accountQuery.error,
                "暂时无法核对创建权限，请刷新后重试。",
            )
          : "当前账号没有新建此类资料的权限。"

    return { accountQuery, canCreate, createBlockedReason }
}
