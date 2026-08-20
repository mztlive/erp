"use client"

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
    surfacePanelClassName,
} from "@/components/business"
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
    ReconciliationDifference,
} from "@/features/mall-sync/types"

type MallSyncReconciliationViewProps = {
    data: MallSyncPageView | undefined
    diffColumns: ColumnDef<ReconciliationDifference>[]
    firstPhase: boolean
    onPullDifference: (externalOrderNo: string) => void
}

export function MallSyncReconciliationView({
    data,
    diffColumns,
    firstPhase,
    onPullDifference,
}: MallSyncReconciliationViewProps) {
    return (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.3fr)_minmax(18rem,1fr)]">
            {data?.reconciliation ? (
                <>
                    <div className="space-y-3">
                        <Card size="sm" className={surfacePanelClassName}>
                            <CardHeader className="border-b border-grid">
                                <CardTitle>
                                    {data.reconciliation.jobNo}
                                </CardTitle>
                                <CardDescription>
                                    {data.reconciliation.boundaryLabel} · 商城{" "}
                                    {data.reconciliation.mallCount} / ERP{" "}
                                    {data.reconciliation.erpCount} · 差异{" "}
                                    {data.reconciliation.differenceCount}
                                </CardDescription>
                            </CardHeader>
                        </Card>
                        <BusinessTableFrame
                            title="核对差异"
                            description="比较完整商业数据标识，只产生差异与任务，不直接覆盖记录。"
                            table={
                                <DataTable
                                    data={data.reconciliation.differences}
                                    columns={diffColumns}
                                    getRowId={(r) => r.differenceId}
                                    rowCount={
                                        data.reconciliation.differences.length
                                    }
                                    layout="flush"
                                    density="compact"
                                />
                            }
                        />
                    </div>
                    {data.selectedDifference ? (
                        <Card size="sm" className={surfacePanelClassName}>
                            <CardHeader className="border-b border-grid">
                                <CardTitle className="font-mono text-base">
                                    {data.selectedDifference.externalOrderNo}
                                </CardTitle>
                                <CardDescription>
                                    {data.selectedDifference.differenceTypeLabel}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-2 text-sm">
                                <BusinessStatusBadge
                                    context="detail"
                                    label={data.selectedDifference.statusLabel}
                                    tone={data.selectedDifference.statusTone}
                                />
                                <p>{data.selectedDifference.impactSummary}</p>
                                {data.selectedDifference.erpSalesOrderNo ? (
                                    <p>
                                        ERP 销售单{" "}
                                        {data.selectedDifference.erpSalesOrderNo}
                                    </p>
                                ) : null}
                                {firstPhase ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        onClick={() =>
                                            onPullDifference(
                                                data.selectedDifference!
                                                    .externalOrderNo,
                                            )
                                        }
                                    >
                                        按此单号补拉
                                    </Button>
                                ) : null}
                            </CardContent>
                        </Card>
                    ) : null}
                </>
            ) : (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前无核对范围"
                    description="当前没有可核对的差异；清除筛选后重试。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                />
            )}
        </div>
    )
}
