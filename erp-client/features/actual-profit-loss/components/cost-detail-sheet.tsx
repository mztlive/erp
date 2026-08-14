import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"
import type { UseQueryResult } from "@tanstack/react-query"

import {
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import { getErrorMessage } from "@/lib/api/errors"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { CostEntryDetailBody } from "@/features/actual-profit-loss/components/cost-entry-detail-body"
import {
    COVERAGE_STATE_UI,
    type CostEntryDetail,
    type ProfitLossRow,
} from "@/features/actual-profit-loss/types"

export function CostDetailSheet({
    open,
    onOpenChange,
    costDetailRow,
    costEntries,
    selectedCostEntryId,
    selectedEntry,
    onSelectEntry,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    costDetailRow: ProfitLossRow | null
    costEntries: UseQueryResult<CostEntryDetail[]>
    selectedCostEntryId: string | null
    selectedEntry: CostEntryDetail | null
    onSelectEntry: (id: string) => void
}) {
    return (
        <QuickPreviewSheet
            open={open}
            onOpenChange={onOpenChange}
            size="detail"
            title="成本记录"
            description="只读 · 不含税；含税仅作税额展示。"
            identity={
                costDetailRow ? (
                    <span>
                        销售单 {costDetailRow.identityLabel} ·{" "}
                        {costDetailRow.customerLabel}
                    </span>
                ) : null
            }
            summary={
                costDetailRow ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={COVERAGE_STATE_UI[costDetailRow.coverageState].label}
                        tone={COVERAGE_STATE_UI[costDetailRow.coverageState].tone}
                        description={costDetailRow.coverageBlockers
                            .map((b) => b.message)
                            .join("；")}
                    />
                ) : null
            }
            footer={
                <div className="flex w-full flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">
                        不可删除成本或直接改金额；更正请走原业务对象变更/冲减。
                    </p>
                    {costDetailRow?.objectId ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={`/sales/orders/${encodeURIComponent(costDetailRow.objectId)}`}
                                    target="_blank"
                                />
                            }
                        >
                            {openWorkspaceLabel("W05")}
                            <ExternalLinkIcon className="ml-1 size-3.5" />
                        </Button>
                    ) : null}
                </div>
            }
        >
            <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
                {costDetailRow && costDetailRow.coverageBlockers.length > 0 ? (
                    <Alert variant="warning">
                        <AlertTitle>成本缺口原因</AlertTitle>
                        <AlertDescription>
                            <ul className="list-disc pl-4">
                                {costDetailRow.coverageBlockers.map((b) => (
                                    <li key={b.code}>{b.message}</li>
                                ))}
                            </ul>
                        </AlertDescription>
                    </Alert>
                ) : null}

                {costEntries.isPending ? (
                    <Skeleton className="h-40 w-full" />
                ) : costEntries.isError ? (
                    <Alert variant="destructive">
                        <AlertTitle>成本记录加载失败</AlertTitle>
                        <AlertDescription>
                            {getErrorMessage(
                                costEntries.error,
                                "未能读取本条销售单的成本记录。请重试；不影响已展示金额。",
                            )}
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                className="ml-2"
                                onClick={() => void costEntries.refetch()}
                            >
                                重试
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : costEntries.data && costEntries.data.length > 0 ? (
                    <>
                        <div className="flex flex-wrap gap-2">
                            {costEntries.data.map((entry) => (
                                <Button
                                    key={entry.costEntryId}
                                    type="button"
                                    size="sm"
                                    variant={
                                        selectedCostEntryId ===
                                        entry.costEntryId
                                            ? "default"
                                            : "outline"
                                    }
                                    onClick={() =>
                                        onSelectEntry(entry.costEntryId)
                                    }
                                >
                                    {entry.costTypeLabel} · {entry.stageLabel}
                                </Button>
                            ))}
                        </div>
                        <Separator />
                        {selectedEntry ? (
                            <CostEntryDetailBody entry={selectedEntry} />
                        ) : null}
                    </>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        当前行无可查看的成本记录（无权限或完全未覆盖）。
                    </p>
                )}
            </div>
        </QuickPreviewSheet>
    )
}
