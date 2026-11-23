"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    SearchIcon,
    ShieldCheckIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PERMISSION_CATALOG } from "@/lib/permission-catalog"
import { cn } from "@/lib/utils"
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

function PermissionModuleRow({
    group,
    multiRole,
}: {
    group: PermissionGroupView
    multiRole: boolean
}) {
    return (
        <div className="grid gap-x-6 gap-y-2.5 py-3.5 md:grid-cols-[13rem_minmax(0,1fr)] md:items-baseline">
            <div className="flex min-w-0 items-baseline justify-between gap-2 md:block">
                <h3 className="truncate font-medium text-foreground">
                    {group.name}
                </h3>
                <span className="num shrink-0 text-xs text-muted-foreground md:mt-0.5 md:block">
                    {group.items.length} 项权限
                </span>
            </div>
            <ul className="flex min-w-0 flex-wrap gap-1.5">
                {group.items.map((item) => {
                    const sourceText = item.sources
                        .map((role) => role.name)
                        .join("、")
                    return (
                        <li key={item.code} className="min-w-0">
                            <span
                                title={`${item.label} · 来源角色：${sourceText}`}
                                className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-border/70 bg-surface-sunken px-2.5 py-1 text-xs text-foreground"
                            >
                                <span className="truncate">
                                    {permissionActionLabel(item, group.name)}
                                </span>
                                {multiRole && item.sources.length > 1 ? (
                                    <span className="num shrink-0 text-tiny text-muted-foreground">
                                        {item.sources.length}
                                    </span>
                                ) : null}
                            </span>
                        </li>
                    )
                })}
            </ul>
        </div>
    )
}

export function AccountOperationPermissions({
    account,
    roles,
    canRead,
    canEditRoles = false,
}: {
    account: AdminAccount
    roles: ReturnType<typeof useRolesQuery>
    canRead: boolean
    canEditRoles?: boolean
}) {
    const [search, setSearch] = React.useState("")
    const [selectedRoleId, setSelectedRoleId] = React.useState<string>("all")

    const preview = React.useMemo(
        () => accountPermissionGroups(account, roles.data ?? []),
        [account, roles.data],
    )

    const effectiveRoleId =
        selectedRoleId !== "all" &&
        preview.assigned.some((role) => role.id === selectedRoleId)
            ? selectedRoleId
            : "all"

    const keyword = search.trim().toLocaleLowerCase()
    const groups = React.useMemo(() => {
        return preview.groups
            .map((group) => ({
                ...group,
                items: group.items.filter((item) => {
                    if (
                        effectiveRoleId !== "all" &&
                        !item.sources.some(
                            (role) => role.id === effectiveRoleId,
                        )
                    ) {
                        return false
                    }
                    if (!keyword) return true
                    return [
                        group.name,
                        item.label,
                        ...item.sources.map((role) => role.name),
                    ]
                        .join(" ")
                        .toLocaleLowerCase()
                        .includes(keyword)
                }),
            }))
            .filter((group) => group.items.length > 0)
            .sort((left, right) => groupRank(left.name) - groupRank(right.name))
    }, [effectiveRoleId, keyword, preview.groups])

    const visibleCount = groups.reduce(
        (sum, group) => sum + group.items.length,
        0,
    )
    const hasFilter = Boolean(keyword) || effectiveRoleId !== "all"
    const multiRole = preview.assigned.length > 1

    return (
        <DetailWorkspaceSection
            title="操作权限"
            headingId="account-operation-permissions-title"
            width="full"
            badge={
                canRead && roles.isSuccess && preview.permissionCount > 0 ? (
                    <Badge variant="secondary" className="num">
                        {preview.allPermissions
                            ? "全部权限"
                            : `${preview.permissionCount} 项`}
                    </Badge>
                ) : null
            }
            actions={
                canRead && roles.isSuccess && preview.permissionCount > 0 ? (
                    <InputGroup className="w-full sm:w-72">
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="account-operation-permissions-search"
                            type="search"
                            aria-label="搜索已有操作权限"
                            placeholder="搜索业务模块、操作或角色"
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                        />
                    </InputGroup>
                ) : null
            }
        >
            {!canRead ? (
                <p className="text-muted-foreground">
                    没有查看角色权限的权限，请联系管理员。
                </p>
            ) : roles.isError ? (
                <BusinessFailureState
                    id="account-operation-permissions-retry"
                    error={roles.error}
                    onRetry={() => void roles.refetch()}
                />
            ) : roles.isPending && !roles.data ? (
                <div className="space-y-3" aria-busy="true">
                    <div className="h-9 w-72 animate-pulse rounded-md bg-muted" />
                    <div className="h-14 animate-pulse rounded-md bg-muted/65" />
                    <div className="h-14 animate-pulse rounded-md bg-muted/50" />
                </div>
            ) : (
                <div className="space-y-4">
                    {preview.assigned.length > 0 ? (
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            <div
                                role="group"
                                aria-label="按来源角色筛选"
                                className="flex flex-wrap items-center gap-1.5"
                            >
                                <span className="mr-1 text-xs text-muted-foreground">
                                    来源角色
                                </span>
                                {multiRole ? (
                                    <>
                                        <button
                                            id="account-operation-permissions-role-all"
                                            type="button"
                                            aria-pressed={
                                                effectiveRoleId === "all"
                                            }
                                            onClick={() =>
                                                setSelectedRoleId("all")
                                            }
                                            className={cn(
                                                "inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-primary",
                                                effectiveRoleId === "all"
                                                    ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                    : "bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                                            )}
                                        >
                                            <span>全部角色</span>
                                            <span className="num text-tiny opacity-80">
                                                {preview.allPermissions
                                                    ? "全部"
                                                    : preview.permissionCount}
                                            </span>
                                        </button>
                                        {preview.assigned.map((role) => {
                                            const active =
                                                effectiveRoleId === role.id
                                            const count =
                                                role.permissions.includes(
                                                    "*:*",
                                                ) ||
                                                role.permissions.includes("*")
                                                    ? "全部"
                                                    : new Set(role.permissions)
                                                          .size
                                            return (
                                                <button
                                                    key={role.id}
                                                    id={`account-operation-permissions-role-${toAutomationIdSegment(role.id)}`}
                                                    type="button"
                                                    aria-pressed={active}
                                                    onClick={() =>
                                                        setSelectedRoleId(
                                                            active
                                                                ? "all"
                                                                : role.id,
                                                        )
                                                    }
                                                    className={cn(
                                                        "inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-primary",
                                                        active
                                                            ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                            : "bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                                                    )}
                                                >
                                                    <span>{role.name}</span>
                                                    <span className="num text-tiny opacity-80">
                                                        {count}
                                                    </span>
                                                </button>
                                            )
                                        })}
                                    </>
                                ) : (
                                    preview.assigned.map((role) =>
                                        canEditRoles ? (
                                            <Link
                                                key={role.id}
                                                id={`account-operation-permissions-source-${toAutomationIdSegment(role.id)}`}
                                                href={`/system/roles/${encodeURIComponent(role.id)}/edit?returnTo=${encodeURIComponent(`/system/accounts/${account.id}`)}`}
                                                className="inline-flex h-7 items-center gap-1 rounded-md bg-muted/65 px-2.5 text-xs font-medium text-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-primary"
                                            >
                                                <span>{role.name}</span>
                                                <ArrowUpRightIcon
                                                    className="size-3 text-muted-foreground"
                                                    aria-hidden="true"
                                                />
                                            </Link>
                                        ) : (
                                            <span
                                                key={role.id}
                                                className="inline-flex h-7 items-center rounded-md bg-muted/65 px-2.5 text-xs font-medium text-foreground"
                                            >
                                                {role.name}
                                            </span>
                                        ),
                                    )
                                )}
                            </div>
                            {hasFilter ? (
                                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                    <span className="num">
                                        匹配 {visibleCount} 项（{groups.length}{" "}
                                        个模块）
                                    </span>
                                    <Button
                                        id="account-operation-permissions-clear-filter"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={() => {
                                            setSearch("")
                                            setSelectedRoleId("all")
                                        }}
                                    >
                                        <XIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        重置筛选
                                    </Button>
                                </div>
                            ) : null}
                        </div>
                    ) : null}

                    {preview.missingRoleCount > 0 ? (
                        <Alert variant="warning">
                            <AlertTitle>部分角色信息未返回</AlertTitle>
                            <AlertDescription>
                                有 {preview.missingRoleCount}{" "}
                                个角色信息未返回，以下仅展示已读取的权限，请联系管理员核对。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {preview.allPermissions ? (
                        <Alert variant="info">
                            <ShieldCheckIcon aria-hidden="true" />
                            <AlertTitle>拥有全部模块操作权限</AlertTitle>
                            <AlertDescription>
                                由「
                                {preview.assigned
                                    .filter(
                                        (role) =>
                                            role.permissions.includes("*:*") ||
                                            role.permissions.includes("*"),
                                    )
                                    .map((role) => role.name)
                                    .join("、")}
                                」授予全部功能操作权限；具体可查看和处理的业务单据仍遵循「数据范围」与审批条件。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {preview.permissionCount > 0 ? (
                        <>
                            {groups.length > 0 ? (
                                <div className="divide-y divide-border/60 border-y border-border/60">
                                    {groups.map((group) => (
                                        <PermissionModuleRow
                                            key={group.name}
                                            group={group}
                                            multiRole={
                                                multiRole &&
                                                effectiveRoleId === "all"
                                            }
                                        />
                                    ))}
                                </div>
                            ) : (
                                <div className="flex flex-col items-start gap-2.5 border-y border-border/60 py-8">
                                    <p
                                        role="status"
                                        className="text-body-compact text-muted-foreground"
                                    >
                                        {hasFilter
                                            ? "没有符合当前筛选条件的操作权限。"
                                            : "尚未读取到可展示的权限。"}
                                    </p>
                                    {hasFilter ? (
                                        <Button
                                            id="account-operation-permissions-empty-clear"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => {
                                                setSearch("")
                                                setSelectedRoleId("all")
                                            }}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : null}
                                </div>
                            )}
                        </>
                    ) : (
                        <p className="border-y border-border/60 py-8 text-muted-foreground">
                            {preview.missingRoleCount > 0
                                ? "尚未读取到操作权限，当前无法确认完整授权。"
                                : account.role_ids.length > 0
                                  ? "当前角色尚未授予操作权限。"
                                  : "尚未分配角色，暂无操作权限。"}
                        </p>
                    )}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}
