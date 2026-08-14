"use client"

import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
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
import { Separator } from "@/components/ui/separator"
import type {
    MallSnapshotRow,
    MallSyncJobRow,
    MallSyncPageView,
    MallSyncViewName,
    ReconciliationDifference,
} from "@/features/mall-sync/types"
import { JOB_ERROR_CLASS_LABEL } from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"
import { surfacePanelClassName } from "@/components/business"
import { versionText } from "@/lib/ui-text"

type PatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean },
) => void

type MallSyncReadViewsProps = {
    view: MallSyncViewName
    context: MallSyncPageView["context"] | undefined
    ownership: MallSyncPageView["context"]["ownership"] | undefined
    data: MallSyncPageView | undefined
    pageJobs: MallSyncJobRow[]
    jobColumns: ColumnDef<MallSyncJobRow>[]
    snapshotColumns: ColumnDef<MallSnapshotRow>[]
    diffColumns: ColumnDef<ReconciliationDifference>[]
    pagination: PaginationState
    onPaginationChange: (
        next: PaginationState | ((current: PaginationState) => PaginationState),
    ) => void
    retryPending: boolean
    onRetryJob: () => void
    patchUrl: PatchUrl
    firstPhase: boolean
    sealed: boolean
    onPullDifference: (externalOrderNo: string) => void
}

function MallSyncReadViews({
    view,
    context,
    ownership,
    data,
    pageJobs,
    jobColumns,
    snapshotColumns,
    diffColumns,
    pagination,
    onPaginationChange,
    retryPending,
    onRetryJob,
    patchUrl,
    firstPhase,
    sealed,
    onPullDifference,
}: MallSyncReadViewsProps) {
    return (
        <>
            {view === "overview" ? (
                <div className="grid gap-4 lg:grid-cols-2">
                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30">
                            <CardTitle>运行摘要</CardTitle>
                            <CardDescription>
                                同步进度仅证明来源数据已捕获，不证明映射或应收已成功。
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-2 text-sm">
                            <div className="flex justify-between gap-2">
                                <span className="text-muted-foreground">
                                    当前同步进度
                                </span>
                                <span className="num text-xs">
                                    {context?.freshness.currentWatermark
                                        ? formatDateTime(
                                              context.freshness
                                                  .currentWatermark,
                                              "default",
                                          )
                                        : "—"}
                                </span>
                            </div>
                            <div className="flex justify-between gap-2">
                                <span className="text-muted-foreground">
                                    最近成功
                                </span>
                                <span>
                                    {formatDateTime(
                                        context?.freshness
                                            .latestSuccessfulJobAt,
                                        "default",
                                    )}
                                </span>
                            </div>
                            <div className="flex justify-between gap-2">
                                <span className="text-muted-foreground">
                                    来源数据更新时间
                                </span>
                                <span>
                                    {formatDateTime(
                                        context?.freshness.sourceSafeTime,
                                        "default",
                                    )}
                                </span>
                            </div>
                            <div className="flex justify-between gap-2">
                                <span className="text-muted-foreground">
                                    主责数量
                                </span>
                                <span>
                                    商城 {ownership?.mallOwnedOrderCount ?? "—"}{" "}
                                    · ERP {ownership?.erpOwnedOrderCount ?? "—"}
                                </span>
                            </div>
                            <Separator />
                            <p className="text-muted-foreground">
                                同步失败不阻塞商城销售/制卡/绑定/激活/消费；差异在
                                ERP 侧处理，无「手工补建销售单」入口。
                            </p>
                        </CardContent>
                    </Card>
                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30">
                            <CardTitle>最近同步任务</CardTitle>
                        </CardHeader>
                        <CardContent className="space-y-2">
                            {(data?.jobs ?? []).slice(0, 4).map((job) => (
                                <button
                                    key={job.jobId}
                                    type="button"
                                    className="flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-sm hover:bg-accent/50"
                                    onClick={() =>
                                        patchUrl({
                                            view: "jobs",
                                            jobId: job.jobId,
                                        })
                                    }
                                >
                                    <span className="font-medium">
                                        {job.jobNo}
                                    </span>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={job.statusLabel}
                                        tone={job.statusTone}
                                    />
                                </button>
                            ))}
                            {(data?.jobs ?? []).length === 0 ? (
                                <p className="text-sm text-muted-foreground">
                                    暂无同步任务。
                                </p>
                            ) : null}
                        </CardContent>
                    </Card>
                </div>
            ) : null}

            {view === "jobs" ? (
                <div className="grid gap-4 xl:grid-cols-[minmax(0,1.4fr)_minmax(18rem,1fr)]">
                    <BusinessTableFrame
                        title="同步任务"
                        description="基线 / 增量 / 单号补拉。同步进度不因映射失败回退。"
                        table={
                            <DataTable
                                data={pageJobs}
                                columns={jobColumns}
                                getRowId={(r) => r.jobId}
                                rowCount={data?.jobs.length ?? 0}
                                pagination={pagination}
                                onPaginationChange={onPaginationChange}
                                layout="flush"
                                density="compact"
                            />
                        }
                    />
                    {data?.selectedJob ? (
                        <Card size="sm" className={surfacePanelClassName}>
                            <CardHeader className="border-b border-border/30">
                                <CardTitle className="text-base">
                                    {data.selectedJob.jobNo}
                                </CardTitle>
                                <CardDescription>
                                    {data.selectedJob.jobTypeLabel} ·{" "}
                                    {data.selectedJob.triggeredBy}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 text-sm">
                                <BusinessStatusBadge
                                    context="detail"
                                    label={data.selectedJob.statusLabel}
                                    tone={data.selectedJob.statusTone}
                                />
                                {data.selectedJob.impactSummary ? (
                                    <p>{data.selectedJob.impactSummary}</p>
                                ) : null}
                                {data.selectedJob.errorClass ? (
                                    <p className="text-muted-foreground">
                                        错误分类：
                                        {JOB_ERROR_CLASS_LABEL[
                                            data.selectedJob.errorClass
                                        ] ?? data.selectedJob.errorClass}
                                    </p>
                                ) : null}
                                <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                                    <span>
                                        游标前{" "}
                                        {data.selectedJob.cursorBefore
                                            ? formatDateTime(
                                                  data.selectedJob.cursorBefore,
                                                  "default",
                                              )
                                            : "—"}
                                    </span>
                                    <span>
                                        游标后{" "}
                                        {data.selectedJob.cursorAfter
                                            ? formatDateTime(
                                                  data.selectedJob.cursorAfter,
                                                  "default",
                                              )
                                            : "—"}
                                    </span>
                                </div>
                                {data.selectedJob.allowedActions.includes(
                                    "RETRY_FAILED_JOB",
                                ) ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        disabled={retryPending}
                                        onClick={onRetryJob}
                                    >
                                        重试失败任务
                                    </Button>
                                ) : null}
                                {data.selectedJob.actionBlockers.map((b) => (
                                    <p
                                        key={b.code}
                                        className="text-xs text-warning-soft-foreground"
                                    >
                                        {b.message}
                                    </p>
                                ))}
                            </CardContent>
                        </Card>
                    ) : null}
                </div>
            ) : null}

            {view === "snapshots" ? (
                <div className="grid gap-4 xl:grid-cols-[minmax(0,1.3fr)_minmax(18rem,1fr)]">
                    <BusinessTableFrame
                        title="来源数据"
                        description="仅白名单商业字段。不含玩法、卡号、卡密、绑定手机、连接或密钥。"
                        table={
                            <DataTable
                                data={data?.snapshots ?? []}
                                columns={snapshotColumns}
                                getRowId={(r) => r.snapshotId}
                                rowCount={(data?.snapshots ?? []).length}
                                layout="flush"
                                density="compact"
                            />
                        }
                    />
                    {data?.selectedSnapshot ? (
                        <Card size="sm" className={surfacePanelClassName}>
                            <CardHeader className="border-b border-border/30">
                                <CardTitle className="font-mono text-base">
                                    {data.selectedSnapshot.externalOrderNo}
                                </CardTitle>
                                <CardDescription>
                                    {versionText.version}{" "}
                                    {data.selectedSnapshot.contentHashShort} ·
                                    任务 {data.selectedSnapshot.syncJobNo}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-2">
                                <Badge variant="outline">
                                    {data.selectedSnapshot.mappingStatusLabel}
                                </Badge>
                                {data.selectedSnapshot.conflictFlags.length >
                                0 ? (
                                    <Alert variant="warning">
                                        <AlertTitle>冲突标记</AlertTitle>
                                        <AlertDescription>
                                            {data.selectedSnapshot.conflictFlags.join(
                                                "、",
                                            )}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                                <dl className="space-y-1.5 text-sm">
                                    {data.selectedSnapshot.whitelistFields.map(
                                        (f) => (
                                            <div
                                                key={f.field}
                                                className="flex justify-between gap-2 border-b border-dashed border-border/60 py-1"
                                            >
                                                <dt className="text-muted-foreground">
                                                    {f.label}
                                                </dt>
                                                <dd className="text-right font-medium">
                                                    {f.value}
                                                </dd>
                                            </div>
                                        ),
                                    )}
                                </dl>
                            </CardContent>
                        </Card>
                    ) : (
                        <BusinessEmptyState
                            kind="no-data"
                            title="选择结果"
                            description="从左侧列表选择一条记录"
                            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                        />
                    )}
                </div>
            ) : null}

            {view === "reconciliation" ? (
                <div className="grid gap-4 xl:grid-cols-[minmax(0,1.3fr)_minmax(18rem,1fr)]">
                    {data?.reconciliation ? (
                        <>
                            <div className="space-y-3">
                                <Card
                                    size="sm"
                                    className={surfacePanelClassName}
                                >
                                    <CardHeader className="border-b border-border/30">
                                        <CardTitle>
                                            {data.reconciliation.jobNo}
                                        </CardTitle>
                                        <CardDescription>
                                            {data.reconciliation.boundaryLabel}{" "}
                                            · 商城{" "}
                                            {data.reconciliation.mallCount} /
                                            ERP {data.reconciliation.erpCount} ·
                                            差异{" "}
                                            {
                                                data.reconciliation
                                                    .differenceCount
                                            }
                                        </CardDescription>
                                    </CardHeader>
                                </Card>
                                <BusinessTableFrame
                                    title="核对差异"
                                    description="比较完整商业数据标识，只产生差异与任务，不直接覆盖记录。"
                                    table={
                                        <DataTable
                                            data={
                                                data.reconciliation.differences
                                            }
                                            columns={diffColumns}
                                            getRowId={(r) => r.differenceId}
                                            rowCount={
                                                data.reconciliation.differences
                                                    .length
                                            }
                                            layout="flush"
                                            density="compact"
                                        />
                                    }
                                />
                            </div>
                            {data.selectedDifference ? (
                                <Card
                                    size="sm"
                                    className={surfacePanelClassName}
                                >
                                    <CardHeader className="border-b border-border/30">
                                        <CardTitle className="font-mono text-base">
                                            {
                                                data.selectedDifference
                                                    .externalOrderNo
                                            }
                                        </CardTitle>
                                        <CardDescription>
                                            {
                                                data.selectedDifference
                                                    .differenceTypeLabel
                                            }
                                        </CardDescription>
                                    </CardHeader>
                                    <CardContent className="space-y-2 text-sm">
                                        <BusinessStatusBadge
                                            context="detail"
                                            label={
                                                data.selectedDifference
                                                    .statusLabel
                                            }
                                            tone={
                                                data.selectedDifference
                                                    .statusTone
                                            }
                                        />
                                        <p>
                                            {
                                                data.selectedDifference
                                                    .impactSummary
                                            }
                                        </p>
                                        {data.selectedDifference
                                            .erpSalesOrderNo ? (
                                            <p>
                                                ERP 销售单{" "}
                                                {
                                                    data.selectedDifference
                                                        .erpSalesOrderNo
                                                }
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
            ) : null}

            {view === "history" ? (
                <div className="space-y-3">
                    {sealed ? (
                        <Alert>
                            <AlertTitle>历史只读</AlertTitle>
                            <AlertDescription>
                                第一期同步已完成归档。请前往执行信息与对账工作区查看后续内容。
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {(data?.history ?? []).map((h) => (
                        <Card
                            key={h.id}
                            size="sm"
                            className={surfacePanelClassName}
                        >
                            <CardHeader className="border-b border-border/30">
                                <CardTitle className="text-base">
                                    {h.title}
                                </CardTitle>
                                <CardDescription>
                                    {formatDateTime(h.recordedAt, "default")}
                                    {h.watermark
                                        ? ` · ${formatDateTime(h.watermark, "default")}`
                                        : ""}
                                    {h.reference ? ` · ${h.reference}` : ""}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="text-sm text-muted-foreground">
                                {h.summary}
                            </CardContent>
                        </Card>
                    ))}
                </div>
            ) : null}
        </>
    )
}

export { MallSyncReadViews }
