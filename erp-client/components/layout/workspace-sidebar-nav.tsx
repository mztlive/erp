"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronsUpDownIcon, LogOutIcon } from "lucide-react"

import { Avatar, AvatarFallback } from "@/components/ui/avatar"
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
import { cn } from "@/lib/utils"
import { useBackgroundJobActiveCountQuery } from "@/features/background-jobs/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
    type WorkspaceNavBadgeKey,
} from "@/lib/workspace-registry"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useInquiryFeedbackCountQuery,
    useProcurementInquiryCountQuery,
} from "@/features/sales-inquiries/hooks/queries"
import { useActionableBookCountQuery } from "@/features/sales-selection/hooks/queries"
import { useWorkspaceInboxCountQuery } from "@/features/workspace/hooks/queries"

type NavBadgeCounts = {
    backgroundJobs: number | undefined
    todo: number
    selectionActionable: number
    inquiryFeedback: number | undefined
    procurementInquiry: number | undefined
}

function badgeCountFor(
    key: WorkspaceNavBadgeKey,
    counts: NavBadgeCounts,
): number | undefined {
    switch (key) {
        case "background-job-active-count":
            return counts.backgroundJobs
        case "todo-count":
            return counts.todo
        case "selection-actionable-count":
            return counts.selectionActionable
        case "inquiry-feedback-count":
            return counts.inquiryFeedback
        case "procurement-inquiry-count":
            return counts.procurementInquiry
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
                        <Avatar size="sm" className="size-7 rounded-md">
                            <AvatarFallback className="rounded-md bg-zinc-900 font-semibold text-zinc-100 text-xs dark:bg-zinc-100 dark:text-zinc-900">
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
                            id="workspace-sidebar-my-transfers"
                            onClick={() =>
                                router.push("/account/data-transfers")
                            }
                        >
                            我的文件任务
                        </DropdownMenuItem>
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

    const todoCountQuery = useWorkspaceInboxCountQuery(
        hasPermission(permissions, "work_item:list"),
    )
    const canListSelectionBooks = hasPermission(
        permissions,
        "sales_selection_booklet:list",
    )
    const selectionCountQuery = useActionableBookCountQuery(
        canListSelectionBooks,
    )
    const inquiryFeedbackCountQuery = useInquiryFeedbackCountQuery(
        hasPermission(permissions, "sales_inquiry:list"),
    )
    const procurementInquiryCountQuery = useProcurementInquiryCountQuery(
        hasPermission(permissions, "procurement_inquiry:list"),
    )
    const backgroundJobCountQuery = useBackgroundJobActiveCountQuery(
        hasPermission(permissions, "background_job:list"),
    )
    const counts: NavBadgeCounts = {
        backgroundJobs: backgroundJobCountQuery.isError
            ? undefined
            : backgroundJobCountQuery.data,
        todo: todoCountQuery.data?.mine ?? 0,
        selectionActionable: selectionCountQuery.data ?? 0,
        inquiryFeedback: inquiryFeedbackCountQuery.isError
            ? undefined
            : inquiryFeedbackCountQuery.data,
        procurementInquiry: procurementInquiryCountQuery.isError
            ? undefined
            : procurementInquiryCountQuery.data,
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
                                const badgeQuery =
                                    item.badge === "inquiry-feedback-count"
                                        ? inquiryFeedbackCountQuery
                                        : item.badge ===
                                            "procurement-inquiry-count"
                                          ? procurementInquiryCountQuery
                                          : item.badge ===
                                              "background-job-active-count"
                                            ? backgroundJobCountQuery
                                            : undefined
                                const badgeError = badgeQuery?.isError
                                    ? `${item.label}数量加载失败：${getErrorMessage(badgeQuery.error, "请稍后重试")}`
                                    : undefined
                                const badgeLabel =
                                    item.badge === "inquiry-feedback-count"
                                        ? `${badgeCount} 张询价单有新反馈待处理`
                                        : item.badge ===
                                            "procurement-inquiry-count"
                                          ? `${badgeCount} 张采购询价待处理`
                                          : item.badge ===
                                              "background-job-active-count"
                                            ? `${badgeCount} 项后台任务尚未完成`
                                            : `${badgeCount} 项待处理`

                                return (
                                    <SidebarMenuItem
                                        key={`${group.label}-${item.href}`}
                                    >
                                        <SidebarMenuButton
                                            id={`workspace-sidebar-nav-${toAutomationIdSegment(item.href)}`}
                                            data-workspace-nav={toAutomationIdSegment(
                                                item.href,
                                            )}
                                            isActive={isActive}
                                            tooltip={item.label}
                                            render={<Link href={item.href} />}
                                        >
                                            <Icon aria-hidden="true" />
                                            <span>{item.label}</span>
                                            {badgeError ||
                                            (badgeCount !== undefined &&
                                                badgeCount > 0) ? (
                                                <span
                                                    data-workspace-nav-badge=""
                                                    aria-label={
                                                        badgeError ?? badgeLabel
                                                    }
                                                    title={
                                                        badgeError ?? badgeLabel
                                                    }
                                                    className={cn(
                                                        "ml-auto flex h-4.5 min-w-4.5 items-center justify-center rounded-full px-1.5 text-[10px] font-semibold tabular-nums group-data-[collapsible=icon]:hidden",
                                                        item.badge ===
                                                            "background-job-active-count" &&
                                                            !badgeError
                                                            ? "bg-success text-success-foreground"
                                                            : "bg-destructive text-destructive-foreground",
                                                    )}
                                                >
                                                    {badgeError
                                                        ? "!"
                                                        : badgeCount}
                                                </span>
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
