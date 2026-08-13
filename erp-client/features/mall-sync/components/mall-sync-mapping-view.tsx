"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessDiffPanel,
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
    SequentialProcessBar,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    MallSyncPageView,
    MappingTaskView,
} from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

type LeaseStatus = "active" | "unclaimed" | "lost" | "released"

type MallSyncMappingViewProps = {
    data: MallSyncPageView | undefined
    mappingTask: MappingTaskView | undefined
    mappingColumns: ColumnDef<MappingTaskView>[]
    selectedCandidateId: string | null
    onSelectCandidate: (candidateId: string) => void
    confirmFormContent: React.ReactNode
    mappingIndex: { current: number; total: number }
    leaseStatus: LeaseStatus
    canConfirmMapping: boolean
    reapplyPending: boolean
    onReapply: () => Promise<void>
    onResolveUnknownReapply: () => Promise<void>
    onBackToQueue: () => void
    onConfirm: () => Promise<void>
    onClaim: () => Promise<void>
}

function MallSyncMappingView({
    data,
    mappingTask,
    mappingColumns,
    selectedCandidateId,
    onSelectCandidate,
    confirmFormContent,
    mappingIndex,
    leaseStatus,
    canConfirmMapping,
    reapplyPending,
    onReapply,
    onResolveUnknownReapply,
    onBackToQueue,
    onConfirm,
    onClaim,
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
                            density="compact"
                        />
                    }
                />

                {mappingTask ? (
                    <div className="space-y-3">
                        <Card size="sm" className={surfacePanelClassName}>
                            <CardHeader className="space-y-2 border-b border-border/30">
                                <div className="flex flex-wrap items-center gap-2">
                                    <CardTitle className="text-base">
                                        {mappingTask.mappingTypeLabel}
                                    </CardTitle>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={`映射 · ${mappingTask.mappingTaskStatusLabel}`}
                                        tone={
                                            mappingTask.mappingTaskStatus ===
                                            "RESOLVED"
                                                ? "success"
                                                : "warning"
                                        }
                                    />
                                    {mappingTask.reapplyOperation ? (
                                        <BusinessStatusBadge
                                            context="detail"
                                            label={`归集 · ${mappingTask.reapplyOperation.statusLabel}`}
                                            tone={
                                                mappingTask.reapplyOperation
                                                    .status === "UNKNOWN"
                                                    ? "destructive"
                                                    : mappingTask
                                                            .reapplyOperation
                                                            .status ===
                                                        "SUCCEEDED"
                                                      ? "success"
                                                      : "info"
                                            }
                                        />
                                    ) : (
                                        <Badge variant="outline">
                                            归集 · 未开始
                                        </Badge>
                                    )}
                                </div>
                                <CardDescription>
                                    {mappingTask.externalOrderNo}
                                    {mappingTask.ownerRoutingState ===
                                    "CONFIGURED" ? (
                                        <>
                                            {" "}
                                            · 责任 {mappingTask.ownerRoleLabel}
                                        </>
                                    ) : (
                                        " · 待责任配置"
                                    )}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3">
                                <Alert>
                                    <AlertTitle>确认的是身份关系</AlertTitle>
                                    <AlertDescription>
                                        不是修改来源销售单；相似候选绝不自动确认/合并。
                                    </AlertDescription>
                                </Alert>

                                {mappingTask.ownerRoutingState === "MISSING" ? (
                                    <Alert variant="destructive">
                                        <AlertTitle>责任归属未配置</AlertTitle>
                                        <AlertDescription>
                                            结算主体责任未配置唯一负责角色；领域差异已保存，确认禁用，不向销售与财务同时生成可完成待办。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                {mappingTask.hasConflict ? (
                                    <Alert variant="warning">
                                        <AlertTitle>映射冲突</AlertTitle>
                                        <AlertDescription>
                                            当前谱系与候选并存。请刷新候选并明确确认依据；冲突未解决前确认禁用。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                <p className="text-sm">
                                    <span className="font-medium">
                                        业务影响：
                                    </span>
                                    {mappingTask.impactSummary}
                                </p>

                                <div className="grid gap-3 md:grid-cols-2">
                                    <div>
                                        <h4 className="mb-2 text-sm font-semibold">
                                            来源白名单记录
                                        </h4>
                                        <dl className="space-y-1 text-sm">
                                            {mappingTask.sourceEvidence.map(
                                                (e) => (
                                                    <div
                                                        key={e.field}
                                                        className="flex justify-between gap-2 border-b border-dashed py-1"
                                                    >
                                                        <dt className="text-muted-foreground">
                                                            {e.label}
                                                        </dt>
                                                        <dd className="text-right">
                                                            {e.sensitive
                                                                ? "***"
                                                                : e.value}
                                                        </dd>
                                                    </div>
                                                ),
                                            )}
                                        </dl>
                                    </div>
                                    <div>
                                        <h4 className="mb-2 text-sm font-semibold">
                                            ERP 候选
                                        </h4>
                                        <ul className="space-y-2">
                                            {mappingTask.candidateTargets.map(
                                                (c) => (
                                                    <li key={c.objectId}>
                                                        <button
                                                            type="button"
                                                            disabled={
                                                                c.eligibility !==
                                                                    "ELIGIBLE" ||
                                                                mappingTask.mappingTaskStatus !==
                                                                    "PENDING" ||
                                                                mappingTask.ownerRoutingState ===
                                                                    "MISSING"
                                                            }
                                                            onClick={() =>
                                                                onSelectCandidate(
                                                                    c.objectId,
                                                                )
                                                            }
                                                            className={cn(
                                                                "w-full rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                                                                selectedCandidateId ===
                                                                    c.objectId
                                                                    ? "border-primary bg-accent"
                                                                    : "hover:bg-muted/60",
                                                                c.eligibility !==
                                                                    "ELIGIBLE" &&
                                                                    "opacity-60",
                                                            )}
                                                        >
                                                            <div className="flex items-center justify-between gap-2">
                                                                <span className="font-medium">
                                                                    {c.stableNo}
                                                                </span>
                                                                <Badge
                                                                    variant={
                                                                        c.eligibility ===
                                                                        "ELIGIBLE"
                                                                            ? "secondary"
                                                                            : "outline"
                                                                    }
                                                                >
                                                                    {c.eligibility ===
                                                                    "ELIGIBLE"
                                                                        ? "可选"
                                                                        : "不可用"}
                                                                </Badge>
                                                            </div>
                                                            <p>{c.label}</p>
                                                            <p className="text-xs text-muted-foreground">
                                                                {c.reason}
                                                            </p>
                                                        </button>
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                    </div>
                                </div>

                                {mappingTask.currentTargets.length > 0 ? (
                                    <div>
                                        <h4 className="mb-2 text-sm font-semibold">
                                            当前谱系
                                        </h4>
                                        <ul className="space-y-1 text-sm">
                                            {mappingTask.currentTargets.map(
                                                (t) => (
                                                    <li
                                                        key={`${t.objectId}-${t.validFrom}`}
                                                        className="rounded-md border px-2 py-1"
                                                    >
                                                        {t.stableNo} {t.label} ·{" "}
                                                        {t.relationRole} ·{" "}
                                                        {t.status}
                                                        {t.validTo
                                                            ? ` · 至 ${t.validTo}`
                                                            : ""}
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                    </div>
                                ) : null}

                                {selectedCandidateId &&
                                mappingTask.mappingTaskStatus === "PENDING" ? (
                                    <BusinessDiffPanel
                                        title="确认依据对照"
                                        changes={[
                                            {
                                                id: "identity",
                                                field: "身份关系",
                                                before: "未确认 / 旧谱系",
                                                after:
                                                    mappingTask.candidateTargets.find(
                                                        (c) =>
                                                            c.objectId ===
                                                            selectedCandidateId,
                                                    )?.label ??
                                                    selectedCandidateId,
                                                note: "确认后建立身份对应关系，不改动来源单",
                                            },
                                            {
                                                id: "impact",
                                                field: "业务影响",
                                                before: "未归属",
                                                after: "映射解决 → 待重新归集",
                                                note: mappingTask.impactSummary,
                                            },
                                        ]}
                                    />
                                ) : null}

                                {mappingTask.resolutionHistory.length > 0 ? (
                                    <div>
                                        <h4 className="mb-2 text-sm font-semibold">
                                            处理历史
                                        </h4>
                                        <ul className="space-y-1 text-xs text-muted-foreground">
                                            {mappingTask.resolutionHistory.map(
                                                (h, i) => (
                                                    <li
                                                        key={`${h.handledAt}-${i}`}
                                                    >
                                                        {formatDateTime(
                                                            h.handledAt,
                                                            "default",
                                                        )}{" "}
                                                        · {h.action} ·{" "}
                                                        {h.result} ·{" "}
                                                        {h.handledBy}
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                    </div>
                                ) : null}

                                {confirmFormContent}

                                {mappingTask.mappingTaskStatus ===
                                "RESOLVED" ? (
                                    <div className="space-y-2 rounded-xl border p-3">
                                        <p className="text-sm font-medium">
                                            固定下一步：使用原数据重新归集
                                        </p>
                                        {mappingTask.reapplyOperation
                                            ?.status === "UNKNOWN" ? (
                                            <Alert variant="destructive">
                                                <AlertTitle>
                                                    重新归集结果未知
                                                </AlertTitle>
                                                <AlertDescription>
                                                    映射结论保持已解决，不回滚、不自动下一项。
                                                </AlertDescription>
                                            </Alert>
                                        ) : null}
                                        {mappingTask.reapplyOperation
                                            ?.status === "SUCCEEDED" ? (
                                            <p className="text-sm">
                                                已形成{" "}
                                                <Link
                                                    className="text-primary hover:underline"
                                                    href={`/sales/orders/${mappingTask.reapplyOperation.salesOrderId}`}
                                                >
                                                    {
                                                        mappingTask
                                                            .reapplyOperation
                                                            .salesOrderNo
                                                    }
                                                </Link>
                                                {mappingTask.reapplyOperation
                                                    .receivableResultReference
                                                    ? ` · 应收 ${mappingTask.reapplyOperation.receivableResultReference}`
                                                    : ""}
                                            </p>
                                        ) : (
                                            <div className="flex flex-wrap gap-2">
                                                <Button
                                                    type="button"
                                                    size="sm"
                                                    disabled={reapplyPending}
                                                    onClick={() =>
                                                        void onReapply()
                                                    }
                                                >
                                                    重新归集
                                                </Button>
                                                {mappingTask.reapplyOperation
                                                    ?.status === "UNKNOWN" ? (
                                                    <Button
                                                        type="button"
                                                        size="sm"
                                                        variant="secondary"
                                                        onClick={() =>
                                                            void onResolveUnknownReapply()
                                                        }
                                                    >
                                                        查询处理结果
                                                    </Button>
                                                ) : null}
                                            </div>
                                        )}
                                    </div>
                                ) : null}

                                {mappingTask.actionBlockers.map((b) => (
                                    <p
                                        key={`${b.action}-${b.code}`}
                                        className="text-xs text-warning-soft-foreground"
                                    >
                                        {b.message}
                                    </p>
                                ))}
                            </CardContent>
                        </Card>

                        {mappingTask.ownerRoutingState === "CONFIGURED" &&
                        mappingTask.mappingTaskStatus === "PENDING" ? (
                            <SequentialProcessBar
                                current={mappingIndex.current}
                                total={mappingIndex.total}
                                leaseStatus={leaseStatus}
                                processLabel="确认映射"
                                // 没有独立的「并打开下一条」路径：两个 handler 同义
                                showProcessNext={false}
                                processDisabled={!canConfirmMapping}
                                onBack={onBackToQueue}
                                onProcess={() => {
                                    if (canConfirmMapping) void onConfirm()
                                }}
                                onProcessNext={() => {
                                    if (canConfirmMapping) void onConfirm()
                                }}
                                onReclaim={() => void onClaim()}
                            />
                        ) : null}
                    </div>
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
