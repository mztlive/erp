"use client"

import * as React from "react"

import { WorkspaceShell } from "@/components/layout/workspace-shell"
import { useCopies } from "@/features/approval-copies/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useBackgroundJobActiveCountQuery } from "@/features/background-jobs/queries"
import { InboxPopover } from "@/features/notifications/components/inbox-popover"
import {
    useInquiryFeedbackCountQuery,
    useProcurementInquiryCountQuery,
} from "@/features/sales-inquiries/hooks/queries"
import { useActionableBookCountQuery } from "@/features/sales-selection/hooks/queries"
import { listWorkItems } from "@/features/work-items/api"
import type { WorkItemDto } from "@/features/work-items/types"
import { workspaceTypeLabel } from "@/features/workspace/api/work-item-meta"
import { useWorkspaceNavigationCounts } from "@/features/workspace/hooks/queries"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { hasPermission } from "@/lib/permissions"
import {
    WorkspaceShellBridgeProvider,
    type WorkspaceNavBadgeData,
    type WorkspaceProfileSnapshot,
    type WorkspaceQuickSearchTaskItem,
    type WorkspaceQuickSearchTaskParams,
    type WorkspaceShellBridgeValue,
} from "@/lib/workspace-shell-bridge"

function mapWorkItemToQuickSearchItem(
    item: WorkItemDto,
): WorkspaceQuickSearchTaskItem {
    const typeLabel = workspaceTypeLabel(
        item.work_item_type,
        item.business_object_type,
    )
    const counterparty = displayName(item.counterparty_label)
    const objectLabel = displayBusinessText(
        item.business_object_label,
        item.business_object_id,
        item.root_business_object_id,
        item.id,
    )
    const primary = counterparty || objectLabel || typeLabel
    const secondary =
        counterparty && objectLabel && objectLabel !== counterparty
            ? objectLabel
            : undefined
    return {
        id: item.id,
        primary,
        secondary,
        typeLabel,
        searchMatches: item.search_matches,
    }
}

async function searchWorkspaceQuickTasks({
    query,
    limit,
}: WorkspaceQuickSearchTaskParams): Promise<
    readonly WorkspaceQuickSearchTaskItem[]
> {
    const page = await listWorkItems({
        scope: "mine",
        query,
        timezone: "Asia/Shanghai",
        sort: "priority_due",
        page: 1,
        pageSize: limit,
    })
    return page.items.map(mapWorkItemToQuickSearchItem)
}

/** 仅在顶层宿主工作区挂载：向纯 layout 壳层注入具体业务 feature 的查询与顶栏收件箱。 */
function WorkspaceHostBridgeProvider({
    children,
}: {
    children: React.ReactNode
}) {
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions

    const navigationCounts = useWorkspaceNavigationCounts(profileQuery.data)
    const copyCountQuery = useCopies()
    const selectionCountQuery = useActionableBookCountQuery(
        hasPermission(permissions, "sales_selection_booklet:list"),
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

    const profile = React.useMemo<WorkspaceProfileSnapshot>(
        () => ({
            data: profileQuery.data,
            isPending: profileQuery.isPending,
            isError: profileQuery.isError,
            isSuccess: profileQuery.isSuccess,
            error: profileQuery.error,
        }),
        [
            profileQuery.data,
            profileQuery.error,
            profileQuery.isError,
            profileQuery.isPending,
            profileQuery.isSuccess,
        ],
    )

    const navBadges = React.useMemo<WorkspaceNavBadgeData>(
        () => ({
            counts: {
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
            },
            errors: {
                "todo-count": {
                    isError: navigationCounts.inbox.isError,
                    error: navigationCounts.inbox.error,
                },
                "approval-started-unread-count": {
                    isError: navigationCounts.started.isError,
                    error: navigationCounts.started.error,
                },
                "copy-unread-count": {
                    isError: copyCountQuery.isError,
                    error: copyCountQuery.error,
                },
                "inquiry-feedback-count": {
                    isError: inquiryFeedbackCountQuery.isError,
                    error: inquiryFeedbackCountQuery.error,
                },
                "procurement-inquiry-count": {
                    isError: procurementInquiryCountQuery.isError,
                    error: procurementInquiryCountQuery.error,
                },
                "background-job-active-count": {
                    isError: backgroundJobCountQuery.isError,
                    error: backgroundJobCountQuery.error,
                },
            },
        }),
        [
            backgroundJobCountQuery.data,
            backgroundJobCountQuery.error,
            backgroundJobCountQuery.isError,
            copyCountQuery.data?.unread_count,
            copyCountQuery.error,
            copyCountQuery.isError,
            inquiryFeedbackCountQuery.data,
            inquiryFeedbackCountQuery.error,
            inquiryFeedbackCountQuery.isError,
            navigationCounts.inbox.data,
            navigationCounts.inbox.error,
            navigationCounts.inbox.isError,
            navigationCounts.started.data,
            navigationCounts.started.error,
            navigationCounts.started.isError,
            procurementInquiryCountQuery.data,
            procurementInquiryCountQuery.error,
            procurementInquiryCountQuery.isError,
            selectionCountQuery.data,
        ],
    )

    const value = React.useMemo<WorkspaceShellBridgeValue>(
        () => ({
            profile,
            navBadges,
            searchQuickTasks: searchWorkspaceQuickTasks,
            inboxComponent: InboxPopover,
        }),
        [navBadges, profile],
    )

    return (
        <WorkspaceShellBridgeProvider value={value}>
            {children}
        </WorkspaceShellBridgeProvider>
    )
}

/** 工作区布局装配入口：将具体业务域适配器绑定到纯 WorkspaceShell。 */
export function AppWorkspaceShell({ children }: { children: React.ReactNode }) {
    return (
        <WorkspaceShell hostProvider={WorkspaceHostBridgeProvider}>
            {children}
        </WorkspaceShell>
    )
}
