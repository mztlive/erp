"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"

import { Badge } from "@/components/ui/badge"
import {
    SidebarGroup,
    SidebarGroupContent,
    SidebarGroupLabel,
    SidebarMenu,
    SidebarMenuButton,
    SidebarMenuItem,
} from "@/components/ui/sidebar"
import { isNavItemActive } from "@/lib/nav-active"
import { getErrorMessage } from "@/lib/api/errors"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
    type WorkspaceNavBadgeKey,
} from "@/lib/workspace-registry"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useFulfillmentCountQuery } from "@/features/fulfillment-operations/queries"
import { useUnifiedTaskCountQuery } from "@/features/unified-task-queue/queries"

type NavBadgeCounts = {
    todo: number
    delivery: number
    warehouse: number
}

function badgeCountFor(
    key: WorkspaceNavBadgeKey,
    counts: NavBadgeCounts,
): number | undefined {
    switch (key) {
        case "todo-count":
            return counts.todo
        case "delivery-count":
            return counts.delivery
        case "warehouse-count":
            return counts.warehouse
    }
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

    const todoCountQuery = useUnifiedTaskCountQuery()
    const deliveryCountQuery = useFulfillmentCountQuery("procurement")
    const warehouseCountQuery = useFulfillmentCountQuery("warehouse")
    const counts: NavBadgeCounts = {
        todo: todoCountQuery.data?.mine ?? 0,
        delivery: deliveryCountQuery.data?.pending ?? 0,
        warehouse: warehouseCountQuery.data?.pending ?? 0,
    }

    if (profileQuery.isPending) {
        return (
            <SidebarGroup className="px-1">
                <SidebarGroupLabel className="sr-only">导航</SidebarGroupLabel>
                <SidebarGroupContent>
                    <p className="px-2 py-3 text-xs text-muted-foreground">
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
                    <p className="px-2 py-3 text-xs text-muted-foreground">
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
                    <p className="px-2 py-3 text-xs text-muted-foreground">
                        当前账号暂无可用菜单
                    </p>
                </SidebarGroupContent>
            </SidebarGroup>
        )
    }

    return (
        <>
            {navGroups.map((group, index) => (
                <SidebarGroup key={group.label} className="px-1">
                    {/* 透明侧栏不靠分割线区分分组，仅用组标签与间距 */}
                    {index > 0 ? (
                        <SidebarGroupLabel className="mt-2">
                            {group.label}
                        </SidebarGroupLabel>
                    ) : (
                        <SidebarGroupLabel className="sr-only">
                            {group.label}
                        </SidebarGroupLabel>
                    )}
                    <SidebarGroupContent>
                        <SidebarMenu className="gap-1">
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
                                            isActive={isActive}
                                            tooltip={item.label}
                                            render={<Link href={item.href} />}
                                        >
                                            <Icon aria-hidden="true" />
                                            <span>{item.label}</span>
                                            {badgeCount && badgeCount > 0 ? (
                                                <Badge
                                                    variant="secondary"
                                                    className="ml-auto border-0 bg-background/70 group-data-[collapsible=icon]:hidden"
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
