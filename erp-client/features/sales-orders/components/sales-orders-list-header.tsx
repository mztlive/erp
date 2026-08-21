"use client"

import * as React from "react"
import Link from "next/link"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    DataFreshness,
    FormalActionResult,
    PageActions,
    PageHeader,
} from "@/components/business"
import type { SalesOrdersListExportJob } from "@/features/sales-orders/lib/sales-orders-list-csv"

export function SalesOrdersListHeader(props: {
    isError: boolean
    isFetching: boolean
    queriedAt?: string
    exportDisabled: boolean
    onExport: () => void
    exportJob: SalesOrdersListExportJob | null
}) {
    const {
        isError,
        isFetching,
        queriedAt,
        exportDisabled,
        onExport,
        exportJob,
    } = props

    return (
        <>
            <PageHeader
                title="销售单"
                metadata={
                    <DataFreshness
                        updatedAt={
                            isError
                                ? "查询失败"
                                : queriedAt
                                  ? queriedAt.slice(11, 16)
                                  : "正在查询"
                        }
                        dateTime={queriedAt}
                        state={
                            isError
                                ? "failed"
                                : isFetching
                                  ? "syncing"
                                  : queriedAt
                                    ? "fresh"
                                    : "unknown"
                        }
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "create",
                                label: "新建销售单",
                                icon: PlusIcon,
                                render: (
                                    <Link href="/sales/orders?mode=create" />
                                ),
                            },
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled: exportDisabled,
                                onClick: onExport,
                            },
                        ]}
                    />
                }
            />

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
