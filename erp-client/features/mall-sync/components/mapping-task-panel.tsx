"use client"

import type { ReactNode } from "react"
import Link from "next/link"

import {
    BusinessDiffPanel,
    BusinessStatusBadge,
    SequentialProcessBar,
    surfacePanelClassName,
} from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
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
import { MappingCandidateList } from "@/features/mall-sync/components/mapping-candidate-list"
import type { MappingTaskView } from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"

type MappingTaskPanelProps = {
    mappingTask: MappingTaskView
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

export function MappingTaskPanel({
    mappingTask,
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
}: MappingTaskPanelProps) {
    return (
        <div className="space-y-3">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="space-y-2 border-b border-grid">
                    <div className="flex flex-wrap items-center gap-2">
                        <CardTitle className="text-base">
                            {mappingTask.mappingTypeLabel}
                        </CardTitle>
                        <BusinessStatusBadge
                            context="detail"
                            label={`映射 · ${mappingTask.mappingTaskStatusLabel}`}
                            tone={
                                mappingTask.mappingTaskStatus === "RESOLVED"
                                    ? "success"
                                    : "warning"
                            }
                        />
                        {mappingTask.reapplyOperation ? (
                            <BusinessStatusBadge
                                context="detail"
                                label={`归集 · ${mappingTask.reapplyOperation.statusLabel}`}
                                tone={
                                    mappingTask.reapplyOperation.status ===
                                    "UNKNOWN"
                                        ? "destructive"
                                        : mappingTask.reapplyOperation
                                                .status === "SUCCEEDED"
                                          ? "success"
                                          : "info"
                                }
                            />
                        ) : (
                            <Badge variant="outline">归集 · 未开始</Badge>
                        )}
                    </div>
                    <CardDescription>
                        {mappingTask.externalOrderNo}
                        {mappingTask.ownerRoutingState === "CONFIGURED" ? (
                            <> · 责任 {mappingTask.ownerRoleLabel}</>
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
                        <span className="font-medium">业务影响：</span>
                        {mappingTask.impactSummary}
                    </p>

                    <div className="grid gap-3 md:grid-cols-2">
                        <div>
                            <h4 className="mb-2 text-sm font-semibold">
                                来源白名单记录
                            </h4>
                            <dl className="space-y-1 text-sm">
                                {mappingTask.sourceEvidence.map((e) => (
                                    <div
                                        key={e.field}
                                        className="flex justify-between gap-2 border-b border-dashed py-1"
                                    >
                                        <dt className="text-muted-foreground">
                                            {e.label}
                                        </dt>
                                        <dd className="text-right">
                                            {e.sensitive ? "***" : e.value}
                                        </dd>
                                    </div>
                                ))}
                            </dl>
                        </div>
                        <MappingCandidateList
                            candidates={mappingTask.candidateTargets}
                            selectedCandidateId={selectedCandidateId}
                            disabled={
                                mappingTask.mappingTaskStatus !== "PENDING" ||
                                mappingTask.ownerRoutingState === "MISSING"
                            }
                            onSelectCandidate={onSelectCandidate}
                        />
                    </div>

                    {mappingTask.currentTargets.length > 0 ? (
                        <div>
                            <h4 className="mb-2 text-sm font-semibold">
                                当前谱系
                            </h4>
                            <ul className="space-y-1 text-sm">
                                {mappingTask.currentTargets.map((t) => (
                                    <li
                                        key={`${t.objectId}-${t.validFrom}`}
                                        className="rounded-md border px-2 py-1"
                                    >
                                        {t.stableNo} {t.label} ·{" "}
                                        {t.relationRole} · {t.status}
                                        {t.validTo ? ` · 至 ${t.validTo}` : ""}
                                    </li>
                                ))}
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
                                        )?.label ?? selectedCandidateId,
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
                                {mappingTask.resolutionHistory.map((h, i) => (
                                    <li key={`${h.handledAt}-${i}`}>
                                        {formatDateTime(h.handledAt, "default")}{" "}
                                        · {h.action} · {h.result} ·{" "}
                                        {h.handledBy}
                                    </li>
                                ))}
                            </ul>
                        </div>
                    ) : null}

                    {confirmFormContent}

                    {mappingTask.mappingTaskStatus === "RESOLVED" ? (
                        <div className="space-y-2 rounded-xl border p-3">
                            <p className="text-sm font-medium">
                                固定下一步：使用原数据重新归集
                            </p>
                            {mappingTask.reapplyOperation?.status ===
                            "UNKNOWN" ? (
                                <Alert variant="destructive">
                                    <AlertTitle>重新归集结果未知</AlertTitle>
                                    <AlertDescription>
                                        映射结论保持已解决，不回滚、不自动下一项。
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {mappingTask.reapplyOperation?.status ===
                            "SUCCEEDED" ? (
                                <p className="text-sm">
                                    已形成{" "}
                                    <Link
                                        className="text-primary hover:underline"
                                        href={`/sales/orders/${mappingTask.reapplyOperation.salesOrderId}`}
                                    >
                                        {
                                            mappingTask.reapplyOperation
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
                                        onClick={() => void onReapply()}
                                    >
                                        重新归集
                                    </Button>
                                    {mappingTask.reapplyOperation?.status ===
                                    "UNKNOWN" ? (
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
                    responsibilityStatus={responsibilityStatus}
                    responsibilityStatusLabel={
                        mappingTask.workItem.ownerUser
                            ? `当前处理人：${mappingTask.workItem.ownerUser.displayName}`
                            : undefined
                    }
                    processLabel="确认映射"
                    // 没有独立的「并打开下一条」路径：两个 handler 同义
                    showProcessNext={false}
                    processDisabled={!canConfirmMapping}
                    pending={actionPending}
                    onBack={onBackToQueue}
                    onProcess={() => {
                        if (canConfirmMapping) void onConfirm()
                    }}
                    onProcessNext={() => {
                        if (canConfirmMapping) void onConfirm()
                    }}
                />
            ) : null}
        </div>
    )
}
