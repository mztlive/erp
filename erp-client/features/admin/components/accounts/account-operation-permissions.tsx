"use client"

import { useState } from "react"
import { BusinessFailureState } from "@/components/business"
import { Input } from "@/components/ui/input"
import type { useRolesQuery } from "../../hooks/queries"
import { accountPermissionGroups } from "../../lib/account-permission-preview"
import type { AdminAccount } from "../../types"

export function AccountOperationPermissions({
    account,
    roles,
    canRead,
}: {
    account: AdminAccount
    roles: ReturnType<typeof useRolesQuery>
    canRead: boolean
}) {
    const [search, setSearch] = useState("")
    const preview = accountPermissionGroups(account, roles.data ?? [])
    const keyword = search.trim().toLocaleLowerCase()
    const groups = preview.groups
        .map((group) => ({
            ...group,
            items: group.items.filter((item) =>
                [
                    group.name,
                    item.label,
                    ...item.sources.map((role) => role.name),
                ]
                    .join(" ")
                    .toLocaleLowerCase()
                    .includes(keyword),
            ),
        }))
        .filter((group) => group.items.length > 0)

    return (
        <section
            aria-labelledby="account-operation-permissions-title"
            className="space-y-3 border-t border-grid pt-4 text-sm"
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <h2
                    id="account-operation-permissions-title"
                    className="font-semibold"
                >
                    已有操作权限
                </h2>
                {canRead && roles.isSuccess && preview.permissionCount > 0 && (
                    <Input
                        id="account-operation-permissions-search"
                        aria-label="搜索已有操作权限"
                        placeholder="搜索业务、操作或来源角色"
                        value={search}
                        onChange={(event) => setSearch(event.target.value)}
                        className="h-8 w-full sm:w-64"
                    />
                )}
            </div>
            <p className="text-xs text-muted-foreground">
                按已保存的角色汇总，重复权限合并展示。可访问的数据请查看“数据范围”，具体操作仍需满足业务及审批条件。
            </p>
            {!canRead ? (
                <p>没有查看角色权限的权限，请联系管理员。</p>
            ) : roles.isError ? (
                <BusinessFailureState
                    id="account-operation-permissions-retry"
                    error={roles.error}
                    onRetry={() => void roles.refetch()}
                />
            ) : roles.isPending ? (
                <p role="status">正在读取操作权限…</p>
            ) : (
                <>
                    {preview.missingRoleCount > 0 && (
                        <p
                            role="status"
                            className="text-amber-700 dark:text-amber-400"
                        >
                            有 {preview.missingRoleCount}{" "}
                            个角色信息未返回，以下仅展示已读取的权限，请联系管理员核对。
                        </p>
                    )}
                    {preview.permissionCount > 0 ? (
                        <>
                            <p className="text-xs text-muted-foreground">
                                {preview.allPermissions
                                    ? "已授予全部操作权限。"
                                    : `已读取 ${preview.assigned.length} 个角色，共 ${preview.permissionCount} 项操作授权。`}
                            </p>
                            {groups.length ? (
                                <div className="divide-y divide-grid">
                                    {groups.map((group) => (
                                        <div
                                            key={group.name}
                                            className="grid gap-3 py-3 md:grid-cols-[8rem_minmax(0,1fr)]"
                                        >
                                            <h3 className="font-medium">
                                                {group.name}
                                            </h3>
                                            <ul className="grid gap-x-6 gap-y-3 sm:grid-cols-2 xl:grid-cols-3">
                                                {group.items.map((item) => (
                                                    <li
                                                        key={item.code}
                                                        className="min-w-0 space-y-1 break-words"
                                                    >
                                                        <p>{item.label}</p>
                                                        <p className="text-xs text-muted-foreground">
                                                            来源：
                                                            {item.sources
                                                                .map(
                                                                    (role) =>
                                                                        role.name,
                                                                )
                                                                .join("、")}
                                                        </p>
                                                    </li>
                                                ))}
                                            </ul>
                                        </div>
                                    ))}
                                </div>
                            ) : (
                                <p
                                    role="status"
                                    className="py-4 text-muted-foreground"
                                >
                                    没有匹配的权限，请修改搜索内容。
                                </p>
                            )}
                        </>
                    ) : (
                        <p className="text-muted-foreground">
                            {preview.missingRoleCount > 0
                                ? "尚未读取到操作权限，当前无法确认完整授权。"
                                : account.role_ids.length
                                  ? "当前角色尚未授予操作权限。"
                                  : "尚未分配角色，暂无操作权限。"}
                        </p>
                    )}
                </>
            )}
        </section>
    )
}
