"use client"

import * as React from "react"

import {
    BackgroundJobProgress,
    PageActions,
    PageHeader,
    PageScaffold,
    type PageAction,
} from "@/components/business"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ListExportMeta } from "@/features/master-data/hooks/use-master-data-list-export"

export function ListPageFrame({
    title,
    hint,
    metadata,
    headerDensity = "compact",
    banner,
    alerts,
    exportMeta,
    actions,
    metrics,
    resultsLabel,
    resultsHeadingRef,
    loading,
    children,
}: {
    title: string
    hint?: React.ReactNode
    metadata?: React.ReactNode
    headerDensity?: "default" | "compact"
    banner?: React.ReactNode
    alerts?: React.ReactNode
    exportMeta?: ListExportMeta | null
    actions: readonly PageAction[]
    metrics?: React.ReactNode
    resultsLabel: string
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    loading: boolean
    children: React.ReactNode
}) {
    if (loading) {
        return (
            <PageScaffold density={headerDensity}>
                <PageHeader title={title} density={headerDensity} />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density={headerDensity}>
            <PageHeader
                title={title}
                description={hint}
                metadata={metadata}
                density={headerDensity}
                actions={
                    <PageActions
                        actions={actions}
                        size={headerDensity === "default" ? "default" : "sm"}
                    />
                }
            />
            {banner}
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
            {metrics}
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
