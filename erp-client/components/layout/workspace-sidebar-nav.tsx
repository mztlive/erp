"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronsUpDownIcon, LogOutIcon } from "lucide-react"

import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    SidebarGroup,
    SidebarGroupContent,
    SidebarGroupLabel,
    SidebarMenu,
    SidebarMenuButton,
    SidebarMenuItem,
} from "@/components/ui/sidebar"
import { logoutAndRedirect } from "@/components/providers/auth-session-provider"
import { isNavItemActive } from "@/lib/nav-active"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api/errors"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
    type WorkspaceNavBadgeKey,
} from "@/lib/workspace-registry"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWorkspaceInboxCountQuery } from "@/features/workspace/hooks/queries"

type NavBadgeCounts = {
    todo: number
}

function badgeCountFor(
    key: WorkspaceNavBadgeKey,
    counts: NavBadgeCounts,
): number | undefined {
    switch (key) {
        case "todo-count":
            return counts.todo
        case "delivery-count":
        case "warehouse-count":
            return undefined
    }
}

function displayInitial(
    name: string | undefined,
    account: string | undefined,
): string {
    const source = (name || account || "用").trim()
    return source.slice(0, 1).toUpperCase() || "用"
}

export function WorkspaceSidebarAccount() {
    const router = useRouter()
    const queryClient = useQueryClient()
    const profileQuery = useAccountProfileQuery()

    const displayName =
        profileQuery.data?.name || profileQuery.data?.account || "已登录"
    const accountLabel = profileQuery.data?.account || "后台账号"

    return (
        <SidebarMenu>
            <SidebarMenuItem>
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id="workspace-sidebar-account-trigger"
                        render={
                            <SidebarMenuButton
                                size="lg"
                                aria-label="账号菜单"
                            />
                        }
                    >
                        <Avatar size="sm">
                            <AvatarFallback>
                                {displayInitial(
                                    profileQuery.data?.name,
                                    profileQuery.data?.account,
                                )}
                            </AvatarFallback>
                        </Avatar>
                        <span className="grid min-w-0 flex-1 text-left text-sm leading-tight">
                            <span className="truncate font-medium">
                                {displayName}
                            </span>
                            <span className="truncate text-xs text-sidebar-foreground/70">
                                {accountLabel}
                            </span>
                        </span>
                        <ChevronsUpDownIcon
                            className="ml-auto"
                            aria-hidden="true"
                        />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        side="top"
                        align="start"
                        className="min-w-56"
                    >
                        <DropdownMenuGroup>
                            <DropdownMenuLabel>
                                <div className="flex flex-col gap-0.5">
                                    <span className="text-sm font-medium text-foreground">
                                        {displayName}
                                    </span>
                                    <span className="text-xs font-normal text-muted-foreground">
                                        {accountLabel}
                                    </span>
                                </div>
                            </DropdownMenuLabel>
                        </DropdownMenuGroup>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                            id="workspace-sidebar-account-logout"
                            variant="destructive"
                            onClick={() =>
                                logoutAndRedirect(router, queryClient)
                            }
                        >
                            <LogOutIcon aria-hidden="true" />
                            退出登录
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </SidebarMenuItem>
        </SidebarMenu>
    )
}

export function WorkspaceSidebarNav() {
    const pathname = usePathname()
    // 必须订阅 searchParams：同 path 只改 lane 时 pathname 不变，否则高亮卡死
    const searchParams = useSearchParams()
    const search = searchParams.toString()
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const navGroups = React.useMemo(
        () => filterNavGroupsByPermissions(WORKSPACE_NAV_GROUPS, permissions),
        [permissions],
    )
    const allHrefs = React.useMemo(
        () =>
            navGroups.flatMap((group) => group.items.map((item) => item.href)),
        [navGroups],
    )

    const todoCountQuery = useWorkspaceInboxCountQuery()
    const counts: NavBadgeCounts = {
        todo: todoCountQuery.data?.mine ?? 0,
    }

    if (profileQuery.isPending) {
        return (
            <SidebarGroup className="px-1">
                <SidebarGroupLabel className="sr-only">导航</SidebarGroupLabel>
                <SidebarGroupContent>
                    <p className="px-2 py-3 text-xs text-sidebar-foreground/70">
                        加载菜单…
                    </p>
                </SidebarGroupContent>
            </SidebarGroup>
        )
    }

    if (profileQuery.isError) {
        return (
            <SidebarGroup className="px-1">
                <SidebarGroupLabel className="sr-only">导航</SidebarGroupLabel>
                <SidebarGroupContent>
                    <p className="px-2 py-3 text-xs text-sidebar-foreground/70">
                        {getErrorMessage(
                            profileQuery.error,
                            "无法加载权限，菜单暂不可用。",
                        )}
                    </p>
                </SidebarGroupContent>
            </SidebarGroup>
        )
    }

    if (navGroups.length === 0) {
        return (
            <SidebarGroup className="px-1">
                <SidebarGroupLabel className="sr-only">导航</SidebarGroupLabel>
                <SidebarGroupContent>
                    <p className="px-2 py-3 text-xs text-sidebar-foreground/70">
                        当前账号暂无可用菜单
                    </p>
                </SidebarGroupContent>
            </SidebarGroup>
        )
    }

    return (
        <>
            {navGroups.map((group, index) => (
                <SidebarGroup key={group.label} className="gap-1 px-1 py-1">
                    <SidebarGroupLabel
                        className={index > 0 ? "mt-2" : undefined}
                    >
                        {group.label}
                    </SidebarGroupLabel>
                    <SidebarGroupContent>
                        <SidebarMenu className="gap-0.5">
                            {group.items.map((item) => {
                                const Icon = item.icon
                                const isActive = isNavItemActive(
                                    pathname,
                                    item.href,
                                    allHrefs,
                                    search,
                                )
                                const badgeCount = item.badge
                                    ? badgeCountFor(item.badge, counts)
                                    : undefined

                                return (
                                    <SidebarMenuItem
                                        key={`${group.label}-${item.href}`}
                                    >
                                        <SidebarMenuButton
                                            id={`workspace-sidebar-nav-${toAutomationIdSegment(item.href)}`}
                                            isActive={isActive}
                                            tooltip={item.label}
                                            render={<Link href={item.href} />}
                                        >
                                            <Icon aria-hidden="true" />
                                            <span>{item.label}</span>
                                            {badgeCount && badgeCount > 0 ? (
                                                <Badge
                                                    variant="secondary"
                                                    className="ml-auto border-0 bg-card group-data-[collapsible=icon]:hidden"
                                                >
                                                    {badgeCount}
                                                </Badge>
                                            ) : null}
                                        </SidebarMenuButton>
                                    </SidebarMenuItem>
                                )
                            })}
                        </SidebarMenu>
                    </SidebarGroupContent>
                </SidebarGroup>
            ))}
        </>
    )
}
