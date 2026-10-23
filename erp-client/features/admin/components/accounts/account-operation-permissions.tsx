"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { PERMISSION_CATALOG } from "@/lib/permission-catalog"
import type { useRolesQuery } from "../../hooks/queries"
import {
    accountPermissionGroups,
    type AccountPermissionItem,
} from "../../lib/account-permission-preview"
import type { AdminAccount } from "../../types"

type PermissionGroupView = {
    name: string
    items: AccountPermissionItem[]
}

const groupOrder = new Map(
    PERMISSION_CATALOG.map((group, index) => [group.name, index] as const),
)

function groupRank(name: string) {
    if (name === "全部模块") return -1
    if (name === "其它权限") return Number.MAX_SAFE_INTEGER
    return groupOrder.get(name) ?? Number.MAX_SAFE_INTEGER - 1
}

/** 组名已表达业务域，标签只保留动作，减少重复文字。 */
function permissionActionLabel(item: AccountPermissionItem, groupName: string) {
    const prefix = `${groupName} · `
    if (groupName !== "其它权限" && item.label.startsWith(prefix)) {
        return item.label.slice(prefix.length)
    }
    return item.label
}

function PermissionGroupCard({ group }: { group: PermissionGroupView }) {
    return (
        <section className="min-w-0 rounded-xl border border-border/70 bg-card p-4">
            <div className="flex items-center justify-between gap-2 border-b border-border/60 pb-2.5">
                <h3 className="min-w-0 truncate font-medium">{group.name}</h3>
                <span className="num shrink-0 text-xs text-muted-foreground">
                    {group.items.length} 项
                </span>
            </div>
            <ul className="mt-3 flex flex-wrap gap-1.5">
                {group.items.map((item) => (
                    <li key={item.code} className="min-w-0">
                        <span
                            title={`${item.label} · 来源：${item.sources
                                .map((role) => role.name)
                                .join("、")}`}
                            className="inline-flex max-w-full items-center rounded-md bg-muted px-2 py-1 text-xs text-foreground"
                        >
                            <span className="truncate">
                                {permissionActionLabel(item, group.name)}
                            </span>
                        </span>
                    </li>
                ))}
            </ul>
        </section>
    )
}

export function AccountOperationPermissions({
    account,
    roles,
    canRead,
}: {
    account: AdminAccount
    roles: ReturnType<typeof useRolesQuery>
    canRead: boolean
}) {
    const [search, setSearch] = React.useState("")
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
        .sort((left, right) => groupRank(left.name) - groupRank(right.name))
    const groupGridClassName =
        groups.length === 1
            ? "grid gap-4"
            : groups.length === 2
              ? "grid items-start gap-4 md:grid-cols-2"
              : "grid items-start gap-4 md:grid-cols-2 2xl:grid-cols-3"

    return (
        <section
            aria-labelledby="account-operation-permissions-title"
            className="space-y-4 border-t border-grid pt-5 text-sm"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                        <h2
                            id="account-operation-permissions-title"
                            className="font-semibold"
                        >
                            已有操作权限
                        </h2>
                        {canRead && preview.permissionCount > 0 ? (
                            <Badge variant="secondary">
                                {preview.allPermissions
                                    ? "全部"
                                    : `${preview.permissionCount} 项`}
                            </Badge>
                        ) : null}
                    </div>
                    <p className="text-xs leading-5 text-muted-foreground">
                        按已保存角色合并，权限按业务模块分组展示。
                    </p>
                </div>
                {canRead && roles.isSuccess && preview.permissionCount > 0 ? (
                    <InputGroup className="w-full sm:w-72">
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="account-operation-permissions-search"
                            type="search"
                            aria-label="搜索已有操作权限"
                            placeholder="搜索业务、操作或角色"
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                        />
                    </InputGroup>
                ) : null}
            </div>

            {canRead && preview.assigned.length > 0 ? (
                <div className="flex flex-wrap items-center gap-1.5 text-xs">
                    <span className="mr-1 text-muted-foreground">来源角色</span>
                    {preview.assigned.map((role) => (
                        <span
                            key={role.id}
                            className="rounded-md bg-muted px-2 py-1 font-medium text-foreground"
                        >
                            {role.name}
                        </span>
                    ))}
                </div>
            ) : null}

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
                    {preview.missingRoleCount > 0 ? (
                        <p role="status" className="text-xs text-warning">
                            有 {preview.missingRoleCount}{" "}
                            个角色信息未返回，以下仅展示已读取的权限，请联系管理员核对。
                        </p>
                    ) : null}
                    {preview.permissionCount > 0 ? (
                        <>
                            {groups.length ? (
                                <div className={groupGridClassName}>
                                    {groups.map((group) => (
                                        <PermissionGroupCard
                                            key={group.name}
                                            group={group}
                                        />
                                    ))}
                                </div>
                            ) : (
                                <p
                                    role="status"
                                    className="rounded-xl border border-dashed border-border px-4 py-6 text-center text-xs text-muted-foreground"
                                >
                                    {keyword
                                        ? "没有匹配的权限，请修改搜索内容。"
                                        : "尚未读取到可展示的权限。"}
                                </p>
                            )}
                            <p className="text-xs leading-5 text-muted-foreground">
                                可访问的数据见“数据范围”；具体操作仍需满足业务及审批条件。
                            </p>
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
