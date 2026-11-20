"use client"

import * as React from "react"

import { BackgroundJobProgress } from "@/components/business"
import {
    ListWorkspacePage,
    type ListWorkspacePageAction,
    type ListWorkSurfaceProps,
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
    surface,
    children,
}: {
    eyebrow?: string
    title: string
    resource: MasterDataResource
    description?: React.ReactNode
    views?: React.ReactNode
    alerts?: React.ReactNode
    exportMeta?: ListExportMeta | null
    actions: readonly ListWorkspacePageAction[]
    resultsLabel: string
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    loading: boolean
    surface?: ListWorkSurfaceProps
    children?: React.ReactNode
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

    return (
        <ListWorkspacePage
            eyebrow={eyebrow}
            title={title}
            description={description}
            views={views}
            actions={visibleActions}
            alerts={alerts}
            notices={
                canExport && exportMeta ? (
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
                ) : null
            }
            resultsLabel={resultsLabel}
            resultsHeadingRef={resultsHeadingRef}
            loading={loading}
            surface={surface}
        >
            {children}
        </ListWorkspacePage>
    )
}
