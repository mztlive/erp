"use client"

import * as React from "react"

import {
    BackgroundJobProgress,
    PageActions,
    PageScaffold,
    type PageAction,
} from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { masterDataListPermission } from "@/features/master-data/lib/data"
import type { MasterDataResource } from "@/features/master-data/types"
import { hasPermission } from "@/lib/permissions"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ListExportMeta } from "@/features/master-data/hooks/use-master-data-list-export"

export function ListPageFrame({
    eyebrow = "基础资料",
    title,
    resource,
    description,
    views,
    alerts,
    exportMeta,
    actions,
    resultsLabel,
    resultsHeadingRef,
    loading,
    children,
}: {
    eyebrow?: string
    title: string
    resource: MasterDataResource
    description?: React.ReactNode
    views?: React.ReactNode
    alerts?: React.ReactNode
    exportMeta?: ListExportMeta | null
    actions: readonly PageAction[]
    resultsLabel: string
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    loading: boolean
    children: React.ReactNode
}) {
    const account = useAccountProfileQuery()
    const canExport =
        account.isSuccess &&
        hasPermission(
            account.data.permissions,
            masterDataListPermission[resource],
        )
    const visibleActions = canExport
        ? actions
        : actions.filter((action) => action.actionKey !== "export")

    if (loading) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader
                    eyebrow={eyebrow}
                    title={title}
                    description={description}
                />
                <div
                    className="h-10 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
                <div className="h-96 animate-pulse bg-muted" aria-busy />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow={eyebrow}
                title={title}
                description={description}
                views={views}
            >
                <PageActions actions={visibleActions} size="sm" />
            </ListWorkspaceHeader>
            {alerts}
            {canExport && exportMeta ? (
                <BackgroundJobProgress
                    mode="all-or-nothing"
                    status="succeeded"
                    total={exportMeta.rowCount}
                    completed={exportMeta.rowCount}
                    succeeded={exportMeta.rowCount}
                    label={masterDataCopy.exportDone}
                    description={
                        <>
                            按当前筛选导出 {exportMeta.rowCount} 条。任务号{" "}
                            <span className="num">{exportMeta.jobId}</span>
                            。不含无权限查看的敏感信息。
                        </>
                    }
                />
            ) : null}
            <h2
                ref={resultsHeadingRef}
                tabIndex={-1}
                className="sr-only outline-none"
            >
                {resultsLabel}
            </h2>
            {children}
        </PageScaffold>
    )
}
