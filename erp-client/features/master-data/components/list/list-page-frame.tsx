"use client"

import * as React from "react"

import {
    BackgroundJobProgress,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
    type PageAction,
} from "@/components/business"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ListExportMeta } from "@/features/master-data/hooks/use-master-data-list-export"

export function ListPageFrame({
    title,
    currentLabel,
    hint,
    banner,
    alerts,
    exportMeta,
    queriedAt,
    actions,
    metrics,
    resultsLabel,
    resultsHeadingRef,
    loading,
    children,
}: {
    title: string
    currentLabel: string
    hint?: React.ReactNode
    banner?: React.ReactNode
    alerts?: React.ReactNode
    exportMeta?: ListExportMeta | null
    queriedAt?: string
    actions: readonly PageAction[]
    metrics?: React.ReactNode
    resultsLabel: string
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    loading: boolean
    children: React.ReactNode
}) {
    if (loading) {
        return (
            <PageScaffold density="compact">
                <PageHeader title={title} />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={title}
                description={hint}
                breadcrumbs={[
                    { id: "md", label: "基础资料", href: "/master-data" },
                    { id: "resource", label: currentLabel, current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt="刚刚"
                        dateTime={queriedAt ?? ""}
                        state="fresh"
                        label="基础资料列表"
                    />
                }
                actions={<PageActions actions={actions} />}
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
