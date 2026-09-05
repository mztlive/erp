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
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ListExportMeta } from "@/features/master-data/hooks/use-master-data-list-export"

export function ListPageFrame({
    eyebrow = "基础资料",
    title,
    description,
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
    description?: React.ReactNode
    alerts?: React.ReactNode
    exportMeta?: ListExportMeta | null
    actions: readonly PageAction[]
    resultsLabel: string
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    loading: boolean
    children: React.ReactNode
}) {
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
            >
                <PageActions actions={actions} size="sm" />
            </ListWorkspaceHeader>
            {alerts}
            {exportMeta ? (
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
