"use client"

import * as React from "react"
import { usePathname, useSearchParams } from "@/lib/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useCopies } from "@/features/approval-copies/queries"
import { useBackgroundJobActiveCountQuery } from "@/features/background-jobs/queries"
import {
    useInquiryFeedbackCountQuery,
    useProcurementInquiryCountQuery,
} from "@/features/sales-inquiries/hooks/queries"
import { useActionableBookCountQuery } from "@/features/sales-selection/hooks/queries"
import { useWorkspaceNavigationCounts } from "@/features/workspace/hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { isNavItemActive } from "@/lib/nav-active"
import { hasPermission } from "@/lib/permissions"
import { workspaceQueueView } from "@/lib/workspace-queue-routes"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
    type WorkspaceNavBadgeKey,
    type WorkspaceNavDomain,
    type WorkspaceNavGroup,
} from "@/lib/workspace-registry"

export type NavBadgeCounts = {
    backgroundJobs: number | undefined
    started?: number
    copied?: number
    todo: number | undefined
    selectionActionable: number
    inquiryFeedback: number | undefined
    procurementInquiry: number | undefined
}

export function badgeCountFor(
    key: WorkspaceNavBadgeKey,
    counts: NavBadgeCounts,
): number | undefined {
    switch (key) {
        case "background-job-active-count":
            return counts.backgroundJobs
        case "approval-started-unread-count":
            return counts.started
        case "copy-unread-count":
            return counts.copied
        case "todo-count":
            return counts.todo
        case "selection-actionable-count":
            return counts.selectionActionable
        case "inquiry-feedback-count":
            return counts.inquiryFeedback
        case "procurement-inquiry-count":
            return counts.procurementInquiry
    }
}

type BadgeErrorLookup = (
    key: WorkspaceNavBadgeKey | undefined,
    label: string,
) => string | undefined

type WorkspaceSidebarContextValue = {
    navPathname: string
    search: string
    profileStatus: "pending" | "error" | "success"
    profileError: unknown
    navGroups: readonly WorkspaceNavGroup[]
    allHrefs: readonly string[]
    counts: NavBadgeCounts
    badgeErrorFor: BadgeErrorLookup
    domainBadgeFor: (domain: WorkspaceNavDomain) => {
        count: number
        urgent: boolean
    }
    canViewBackgroundJobs: boolean
    canViewSelectionBooklets: boolean
    currentDomain: WorkspaceNavDomain
    routeDomain: WorkspaceNavDomain | null
    availableDomains: readonly WorkspaceNavDomain[]
    setDomain: (domain: WorkspaceNavDomain) => void
}

const DOMAIN_ORDER: readonly WorkspaceNavDomain[] = [
    "work",
    "sales",
    "procurement",
    "finance",
    "analytics",
    "master-data",
    "governance",
]

const WorkspaceSidebarContext =
    React.createContext<WorkspaceSidebarContextValue | null>(null)

export function WorkspaceSidebarProvider({
    children,
}: {
    children: React.ReactNode
}) {
    const pathname = usePathname()
    // 必须订阅 searchParams：同 path 只改 lane 时 pathname 不变，否则高亮卡死
    const searchParams = useSearchParams()
    const returnTo = searchParams.get("returnTo")
    const parentPath = returnTo?.split("?")[0]
    const fromQueue =
        /^\/workspace\/(tasks|approvals|copies)\/[^/]+$/.test(pathname) &&
        parentPath &&
        workspaceQueueView(parentPath)
    const navPathname = fromQueue ? parentPath : pathname
    const search = fromQueue
        ? (returnTo?.split("?")[1] ?? "")
        : searchParams.toString()

    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const accountKind = profileQuery.data?.account_kind

    const navGroups = React.useMemo(
        () =>
            filterNavGroupsByPermissions(WORKSPACE_NAV_GROUPS, permissions)
                .map((group) => ({
                    ...group,
                    items: group.items.filter(
                        (item) =>
                            item.badge !== "copy-unread-count" ||
                            accountKind === "admin",
                    ),
                }))
                .filter((group) => group.items.length > 0),
        [permissions, accountKind],
    )

    const allHrefs = React.useMemo(
        () =>
            navGroups.flatMap((group) => group.items.map((item) => item.href)),
        [navGroups],
    )

    const availableDomains = React.useMemo(
        () =>
            DOMAIN_ORDER.filter((domain) =>
                navGroups.some((group) => group.domain === domain),
            ),
        [navGroups],
    )

    const routeDomain = React.useMemo<WorkspaceNavDomain | null>(() => {
        const matched = navGroups.find((group) =>
            group.items.some((item) =>
                isNavItemActive(navPathname, item.href, allHrefs, search),
            ),
        )
        return matched?.domain ?? null
    }, [allHrefs, navGroups, navPathname, search])

    const routeKey = `${navPathname}?${search}`
    const [manualSelection, setManualSelection] = React.useState<{
        routeKey: string
        domain: WorkspaceNavDomain
    } | null>(null)

    const manualDomain =
        manualSelection && manualSelection.routeKey === routeKey
            ? manualSelection.domain
            : null

    const currentDomain: WorkspaceNavDomain =
        manualDomain && availableDomains.includes(manualDomain)
            ? manualDomain
            : routeDomain && availableDomains.includes(routeDomain)
              ? routeDomain
              : (availableDomains[0] ?? "work")

    const setDomain = React.useCallback(
        (domain: WorkspaceNavDomain) => {
            setManualSelection({ routeKey, domain })
        },
        [routeKey],
    )

    const navigationCounts = useWorkspaceNavigationCounts(profileQuery.data)
    const copyCountQuery = useCopies()
    const canViewSelectionBooklets = hasPermission(
        permissions,
        "sales_selection_booklet:list",
    )
    const selectionCountQuery = useActionableBookCountQuery(
        canViewSelectionBooklets,
    )
    const inquiryFeedbackCountQuery = useInquiryFeedbackCountQuery(
        hasPermission(permissions, "sales_inquiry:list"),
    )
    const procurementInquiryCountQuery = useProcurementInquiryCountQuery(
        hasPermission(permissions, "procurement_inquiry:list"),
    )
    const canViewBackgroundJobs = hasPermission(
        permissions,
        "background_job:list",
    )
    const backgroundJobCountQuery = useBackgroundJobActiveCountQuery(
        canViewBackgroundJobs,
    )

    const counts: NavBadgeCounts = React.useMemo(
        () => ({
            backgroundJobs: backgroundJobCountQuery.isError
                ? undefined
                : backgroundJobCountQuery.data,
            todo: navigationCounts.inbox.isError
                ? undefined
                : navigationCounts.inbox.data,
            started: navigationCounts.started.isError
                ? undefined
                : navigationCounts.started.data,
            copied: copyCountQuery.isError
                ? undefined
                : copyCountQuery.data?.unread_count,
            selectionActionable: selectionCountQuery.data ?? 0,
            inquiryFeedback: inquiryFeedbackCountQuery.isError
                ? undefined
                : inquiryFeedbackCountQuery.data,
            procurementInquiry: procurementInquiryCountQuery.isError
                ? undefined
                : procurementInquiryCountQuery.data,
        }),
        [
            backgroundJobCountQuery.data,
            backgroundJobCountQuery.isError,
            copyCountQuery.data?.unread_count,
            copyCountQuery.isError,
            inquiryFeedbackCountQuery.data,
            inquiryFeedbackCountQuery.isError,
            navigationCounts.inbox.data,
            navigationCounts.inbox.isError,
            navigationCounts.started.data,
            navigationCounts.started.isError,
            procurementInquiryCountQuery.data,
            procurementInquiryCountQuery.isError,
            selectionCountQuery.data,
        ],
    )

    const domainBadgeFor = React.useCallback(
        (domain: WorkspaceNavDomain): { count: number; urgent: boolean } => {
            switch (domain) {
                case "work": {
                    const urgentCount =
                        (counts.todo ?? 0) + (counts.copied ?? 0)
                    const total = urgentCount + (counts.started ?? 0)
                    return { count: total, urgent: urgentCount > 0 }
                }
                case "sales":
                    return {
                        count:
                            (counts.inquiryFeedback ?? 0) +
                            counts.selectionActionable,
                        urgent: false,
                    }
                case "procurement":
                    return {
                        count: counts.procurementInquiry ?? 0,
                        urgent: false,
                    }
                case "governance":
                    return {
                        count: counts.backgroundJobs ?? 0,
                        urgent: false,
                    }
                default:
                    return { count: 0, urgent: false }
            }
        },
        [counts],
    )

    const badgeErrorFor = React.useCallback<BadgeErrorLookup>(
        (key, label) => {
            if (!key) return undefined
            const query =
                key === "todo-count"
                    ? navigationCounts.inbox
                    : key === "approval-started-unread-count"
                      ? navigationCounts.started
                      : key === "copy-unread-count"
                        ? copyCountQuery
                        : key === "inquiry-feedback-count"
                          ? inquiryFeedbackCountQuery
                          : key === "procurement-inquiry-count"
                            ? procurementInquiryCountQuery
                            : key === "background-job-active-count"
                              ? backgroundJobCountQuery
                              : undefined
            return query?.isError
                ? `${label}数量加载失败：${getErrorMessage(query.error, "请稍后重试")}`
                : undefined
        },
        [
            backgroundJobCountQuery,
            copyCountQuery,
            inquiryFeedbackCountQuery,
            navigationCounts.inbox,
            navigationCounts.started,
            procurementInquiryCountQuery,
        ],
    )

    const value = React.useMemo<WorkspaceSidebarContextValue>(
        () => ({
            navPathname,
            search,
            profileStatus: profileQuery.isPending
                ? "pending"
                : profileQuery.isError
                  ? "error"
                  : "success",
            profileError: profileQuery.error,
            navGroups,
            allHrefs,
            counts,
            badgeErrorFor,
            domainBadgeFor,
            canViewBackgroundJobs,
            canViewSelectionBooklets,
            currentDomain,
            routeDomain,
            availableDomains,
            setDomain,
        }),
        [
            allHrefs,
            availableDomains,
            badgeErrorFor,
            canViewBackgroundJobs,
            canViewSelectionBooklets,
            counts,
            currentDomain,
            domainBadgeFor,
            navGroups,
            navPathname,
            profileQuery.error,
            profileQuery.isError,
            profileQuery.isPending,
            routeDomain,
            search,
            setDomain,
        ],
    )

    return (
        <WorkspaceSidebarContext.Provider value={value}>
            {children}
        </WorkspaceSidebarContext.Provider>
    )
}

export function useWorkspaceSidebar() {
    const context = React.useContext(WorkspaceSidebarContext)
    if (!context) {
        throw new Error(
            "useWorkspaceSidebar must be used within WorkspaceSidebarProvider.",
        )
    }
    return context
}
