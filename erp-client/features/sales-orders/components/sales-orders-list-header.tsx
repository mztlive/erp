"use client"

import * as React from "react"
import Link from "next/link"
import { DownloadIcon, LoaderCircleIcon, PlusIcon } from "lucide-react"

import { FormalActionResult, PageActions } from "@/components/business"
import { listWorkspaceStyles as styles } from "@/components/business/list-workspace"
import type { SalesOrdersListExportJob } from "@/features/sales-orders/lib/sales-orders-list-csv"

export function SalesOrdersListHeader(props: {
    isError: boolean
    isFetching: boolean
    queriedAt?: string
    exportDisabled: boolean
    isExporting?: boolean
    onExport: () => void
    exportJob: SalesOrdersListExportJob | null
}) {
    const {
        isError,
        isFetching,
        queriedAt,
        exportDisabled,
        isExporting = false,
        onExport,
        exportJob,
    } = props

    return (
        <>
            <header className={styles.header}>
                <div>
                    <p className={styles.eyebrow}>销售</p>
                    <h1 className={styles.title}>销售单</h1>
                    <p className={styles.description}>
                        查看销售单、履约进度与成交金额。
                        <span className="ml-3 text-xs" role="status">
                            {isError ? (
                                "查询失败"
                            ) : isFetching ? (
                                "正在更新…"
                            ) : queriedAt ? (
                                <time dateTime={queriedAt}>
                                    更新于 {queriedAt.slice(11, 16)}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </p>
                </div>
                <div className={styles.headerActions}>
                    <PageActions
                        actions={[
                            {
                                actionKey: "create",
                                id: "sales-orders-list-header-create",
                                label: "新建销售单",
                                icon: PlusIcon,
                                render: (
                                    <Link href="/sales/orders?mode=create" />
                                ),
                            },
                            {
                                actionKey: "export",
                                id: "sales-orders-list-header-export",
                                label: isExporting ? "导出中…" : "导出",
                                icon: isExporting
                                    ? LoaderCircleIcon
                                    : DownloadIcon,
                                variant: "outline",
                                disabled: exportDisabled,
                                onClick: onExport,
                            },
                        ]}
                    />
                </div>
            </header>

            {exportJob ? (
                <FormalActionResult
                    status="succeeded"
                    title="导出完成"
                    description={`已生成 CSV 文件，共 ${exportJob.rowCount} 行，仅包含当前筛选结果；导出后金额与状态以列表页最新数据为准。`}
                    facts={[
                        {
                            label: "文件",
                            value: exportJob.fileName,
                        },
                        {
                            label: "行数",
                            value: String(exportJob.rowCount),
                        },
                        {
                            label: "导出时间",
                            value: new Date(
                                exportJob.exportedAt,
                            ).toLocaleString("zh-CN"),
                        },
                    ]}
                />
            ) : null}
        </>
    )
}
