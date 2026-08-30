"use client"

import * as React from "react"
import type { ReadonlyURLSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { useMallSyncPageQuery } from "@/features/mall-sync/hooks/queries"
import { useMallSyncColumns } from "@/features/mall-sync/hooks/mall-sync-columns"
import type { MallSyncViewName } from "@/features/mall-sync/types"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { PatchUrl } from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"

export type UseMallSyncPageInput = {
    view: MallSyncViewName
    q: string
    mappingType?: string
    jobId?: string
    snapshotId?: string
    mappingTaskId?: string
    workItemId?: string
    differenceId?: string
    queueContextId: string
    searchParams: ReadonlyURLSearchParams
    patchUrl: PatchUrl
    advanceAfterConfirm?: boolean
    onTaskCompleted?: (workItemId: string) => void
}

export function useMallSyncPageData(input: UseMallSyncPageInput) {
    const {
        view,
        q,
        mappingType,
        jobId,
        snapshotId,
        mappingTaskId,
        workItemId,
        differenceId,
        queueContextId,
        searchParams,
        patchUrl,
    } = input

    const queryInput = React.useMemo(
        () => ({
            view,
            q: q || undefined,
            mappingType: mappingType || undefined,
            jobId,
            snapshotId,
            mappingTaskId,
            workItemId,
            differenceId,
            queueContextId,
            owner: "all" as const,
        }),
        [
            view,
            q,
            mappingType,
            jobId,
            snapshotId,
            mappingTaskId,
            workItemId,
            differenceId,
            queueContextId,
        ],
    )

    const pageQuery = useMallSyncPageQuery(queryInput)
    const profileQuery = useAccountProfileQuery()

    const data = pageQuery.data
    const context = data?.context
    const ownership = context?.ownership
    const stage = ownership?.stage ?? "ARCHIVED"
    const firstPhase = stage === "FIRST_PHASE_MALL_OWNED"
    const sealed = stage === "ARCHIVED"

    const mappingTask = data?.selectedMappingTask
    const mappingIndex = React.useMemo(() => {
        if (!data?.mappingTasks.length || !mappingTask)
            return { current: 0, total: 0 }
        const idx = data.mappingTasks.findIndex(
            (t) => t.mappingTaskId === mappingTask.mappingTaskId,
        )
        return {
            current: idx >= 0 ? idx + 1 : 1,
            total: data.mappingTasks.length,
        }
    }, [data?.mappingTasks, mappingTask])

    // 封存后默认引导 history
    React.useEffect(() => {
        if (
            sealed &&
            view !== "history" &&
            !jobId &&
            !snapshotId &&
            !mappingTaskId
        ) {
            // 不强制 replace 每次；仅当无对象 id 时提示在 banner
        }
    }, [sealed, view, jobId, snapshotId, mappingTaskId])

    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: 0,
        pageSize: 20,
    })

    // 已生效筛选变化（提交 / 清除 / chip 移除 / 前进后退）回第 1 页；
    // 视图与对象定位参数变化不重置分页
    const appliedFilterSignature = React.useMemo(
        () => `${q}\u0000${mappingType ?? ""}`,
        [mappingType, q],
    )
    React.useEffect(() => {
        setPagination((current) =>
            current.pageIndex === 0 ? current : { ...current, pageIndex: 0 },
        )
    }, [appliedFilterSignature])

    const { diffColumns, jobColumns, mappingColumns, snapshotColumns } =
        useMallSyncColumns({ patchUrl, searchParams })
    const responsibilityStatus: ResponsibilityStatus = (() => {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED") return "blocked"
        const workItem = mappingTask.workItem
        if (workItem.status === "COMPLETED") return "completed"
        if (workItem.status === "CLOSED") return "closed"
        if (workItem.processingState === "APPROVAL_BLOCKED") return "blocked"
        return workItem.ownerUser?.id === profileQuery.data?.userid
            ? "assigned_to_me"
            : "assigned_to_other"
    })()

    const canManualSync = firstPhase && !context?.sourceUnavailable
    const manualSyncDisabledReason = !firstPhase
        ? "已封存：无第一期写动作"
        : context?.sourceUnavailable
          ? "来源不可用时不新建推进任务（可重试既有失败）"
          : null

    const pageJobs = React.useMemo(() => {
        const rows = data?.jobs ?? []
        const start = pagination.pageIndex * pagination.pageSize
        return rows.slice(start, start + pagination.pageSize)
    }, [data?.jobs, pagination])

    return {
        pageQuery,
        data,
        context,
        ownership,
        stage,
        firstPhase,
        sealed,
        mappingTask,
        mappingIndex,
        pagination,
        setPagination,
        diffColumns,
        jobColumns,
        mappingColumns,
        snapshotColumns,
        responsibilityStatus,
        canManualSync,
        manualSyncDisabledReason,
        pageJobs,
    }
}

export type MallSyncPageData = ReturnType<typeof useMallSyncPageData>
