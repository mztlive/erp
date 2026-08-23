"use client"

import type { ReactNode } from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { MappingTaskPanel } from "@/features/mall-sync/components/mapping-task-panel"
import type {
    MallSyncPageView,
    MappingTaskView,
} from "@/features/mall-sync/types"

type MallSyncMappingViewProps = {
    data: MallSyncPageView | undefined
    mappingTask: MappingTaskView | undefined
    mappingColumns: ColumnDef<MappingTaskView>[]
    selectedCandidateId: string | null
    onSelectCandidate: (candidateId: string) => void
    confirmFormContent: ReactNode
    mappingIndex: { current: number; total: number }
    responsibilityStatus: ResponsibilityStatus
    canConfirmMapping: boolean
    actionPending: boolean
    reapplyPending: boolean
    onReapply: () => Promise<void>
    onResolveUnknownReapply: () => Promise<void>
    onBackToQueue: () => void
    onConfirm: () => Promise<void>
}

function MallSyncMappingView({
    data,
    mappingTask,
    mappingColumns,
    selectedCandidateId,
    onSelectCandidate,
    confirmFormContent,
    mappingIndex,
    responsibilityStatus,
    canConfirmMapping,
    actionPending,
    reapplyPending,
    onReapply,
    onResolveUnknownReapply,
    onBackToQueue,
    onConfirm,
}: MallSyncMappingViewProps) {
    return (
        <div className="space-y-4">
            {data?.emptyReason === "NO_TASKS" ||
            data?.emptyReason === "FILTER_NO_RESULT" ? (
                <BusinessEmptyState
                    kind={
                        data.emptyReason === "FILTER_NO_RESULT"
                            ? "filter"
                            : "no-tasks"
                    }
                    title={
                        data.emptyReason === "NO_TASKS"
                            ? "当前没有待处理映射"
                            : "筛选无结果"
                    }
                    description={
                        data.emptyReason === "FILTER_NO_RESULT"
                            ? "清除筛选后查看其它任务。"
                            : "新任务到达后刷新"
                    }
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                />
            ) : null}

            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
                <BusinessTableFrame
                    title="映射任务"
                    description="映射状态与重新归集状态分列；责任未配置时不可执行。"
                    table={
                        <DataTable
                            data={data?.mappingTasks ?? []}
                            columns={mappingColumns}
                            getRowId={(r) => r.mappingTaskId}
                            rowCount={(data?.mappingTasks ?? []).length}
                            layout="flush"
                        />
                    }
                />

                {mappingTask ? (
                    <MappingTaskPanel
                        mappingTask={mappingTask}
                        selectedCandidateId={selectedCandidateId}
                        onSelectCandidate={onSelectCandidate}
                        confirmFormContent={confirmFormContent}
                        mappingIndex={mappingIndex}
                        responsibilityStatus={responsibilityStatus}
                        canConfirmMapping={canConfirmMapping}
                        actionPending={actionPending}
                        reapplyPending={reapplyPending}
                        onReapply={onReapply}
                        onResolveUnknownReapply={onResolveUnknownReapply}
                        onBackToQueue={onBackToQueue}
                        onConfirm={onConfirm}
                    />
                ) : (
                    <BusinessEmptyState
                        kind="no-data"
                        title="选择映射任务"
                        description="从左侧列表打开处理区"
                        className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    />
                )}
            </div>
        </div>
    )
}

export { MallSyncMappingView }
