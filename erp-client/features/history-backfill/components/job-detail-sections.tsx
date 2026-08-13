"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { DownloadIcon, ExternalLinkIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
    OptionCombobox,
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
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { HistoryBackfillFact as Fact } from "@/features/history-backfill/components/history-backfill-fact"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import { useHistoryBackfillDetailQuery } from "@/features/history-backfill/hooks/queries"
import type {
    BackfillPipelineStage,
    CostBasis,
    HistoryBackfillItemView,
    ItemResult,
    JobSection,
    MallOrderFactType,
} from "@/features/history-backfill/types"
import {
    COST_BASIS_LABEL,
    FACT_TYPE_LABEL,
    FAILURE_CODE_LABEL,
    ITEM_RESULT_LABEL,
    ITEM_RESULT_TONE,
    PIPELINE_STAGE_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
} from "@/features/history-backfill/types"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import { formatDateTime } from "@/lib/datetime"

function ItemFilters({
    urlState,
    patchUrl,
    section,
}: {
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    section: JobSection
}) {
    const [qDraft, setQDraft] = React.useState(urlState.q ?? "")
    return (
        <div className="flex flex-wrap items-end gap-2 rounded-lg bg-muted/40 p-3">
            {section === "facts" ? (
                <>
                    <div className="space-y-1">
                        <Label className="text-xs">结果</Label>
                        <OptionCombobox
                            value={urlState.result ?? "all"}
                            onValueChange={(v) => {
                                if (v == null) return
                                patchUrl({
                                    result:
                                        v === "all"
                                            ? undefined
                                            : (v as ItemResult),
                                    page: 1,
                                })
                            }}
                            options={[
                                { value: "all", label: "全部结果" },
                                ...(
                                    Object.keys(
                                        ITEM_RESULT_LABEL,
                                    ) as ItemResult[]
                                ).map((r) => ({
                                    value: r,
                                    label: ITEM_RESULT_LABEL[r],
                                })),
                            ]}
                            className="w-[10rem]"
                            size="sm"
                            allowClear={false}
                        />
                    </div>
                    <div className="space-y-1">
                        <Label className="text-xs">记录类型</Label>
                        <OptionCombobox
                            value={urlState.factType ?? "all"}
                            onValueChange={(v) => {
                                if (v == null) return
                                patchUrl({
                                    factType:
                                        v === "all"
                                            ? undefined
                                            : (v as MallOrderFactType),
                                    page: 1,
                                })
                            }}
                            options={[
                                { value: "all", label: "全部五类" },
                                ...(
                                    Object.keys(
                                        FACT_TYPE_LABEL,
                                    ) as MallOrderFactType[]
                                ).map((t) => ({
                                    value: t,
                                    label: FACT_TYPE_LABEL[t],
                                })),
                            ]}
                            className="w-[12rem]"
                            size="sm"
                            allowClear={false}
                        />
                    </div>
                    <div className="space-y-1">
                        <Label className="text-xs">成本口径</Label>
                        <OptionCombobox
                            value={urlState.costBasis ?? "all"}
                            onValueChange={(v) => {
                                if (v == null) return
                                patchUrl({
                                    costBasis:
                                        v === "all"
                                            ? undefined
                                            : (v as CostBasis),
                                    page: 1,
                                })
                            }}
                            options={[
                                { value: "all", label: "全部" },
                                ...(
                                    Object.keys(COST_BASIS_LABEL) as CostBasis[]
                                ).map((b) => ({
                                    value: b,
                                    label: COST_BASIS_LABEL[b],
                                })),
                            ]}
                            className="w-[9rem]"
                            size="sm"
                            allowClear={false}
                        />
                    </div>
                </>
            ) : null}
            <div className="space-y-1">
                <Label className="text-xs">搜索</Label>
                <form
                    className="flex gap-1"
                    onSubmit={(e) => {
                        e.preventDefault()
                        patchUrl({ q: qDraft.trim() || undefined, page: 1 })
                    }}
                >
                    <Input
                        className="h-8 w-[12rem]"
                        value={qDraft}
                        onChange={(e) => setQDraft(e.target.value)}
                        placeholder="商城订单号 / 子单号"
                    />
                    {urlState.q ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                                setQDraft("")
                                patchUrl({ q: undefined, page: 1 })
                            }}
                        >
                            清除
                        </Button>
                    ) : null}
                    <Button type="submit" size="sm" variant="secondary">
                        搜索
                    </Button>
                </form>
            </div>
            <p className="w-full text-xs text-muted-foreground">
                同一商城订单的多笔关键记录分别保留，多次退款/恢复不合并
            </p>
        </div>
    )
}

function OverviewSection({
    job,
}: {
    job: NonNullable<
        Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
    >["job"]
}) {
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>任务身份与范围</CardTitle>
                    <CardDescription>
                        范围起点固定等于必须覆盖起点
                    </CardDescription>
                </CardHeader>
                <CardContent className="grid gap-3 sm:grid-cols-2">
                    <Fact label="切换编号" value={job.cutoverId} mono />
                    <Fact
                        label="必须覆盖起点"
                        value={formatDay(job.requiredHistoryStart)}
                        mono
                    />
                    <Fact
                        label="范围起点"
                        value={formatDay(job.rangeStart)}
                        mono
                    />
                    <Fact
                        label="截止时点"
                        value={formatDay(job.rangeEnd)}
                        mono
                    />
                    <Fact
                        label="覆盖完整"
                        value={job.coverageComplete ? "是" : "否"}
                    />
                    <Fact
                        label="阶段"
                        value={PIPELINE_STAGE_LABEL[job.pipelineStage]}
                    />
                </CardContent>
            </Card>
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>结果记录</CardTitle>
                    <CardDescription>
                        统计由系统统一计算；明细可按页浏览。
                    </CardDescription>
                </CardHeader>
                <CardContent className="grid gap-3 sm:grid-cols-2">
                    <Fact
                        label="来源记录数"
                        value={job.progress.totalCount.toLocaleString("zh-CN")}
                    />
                    <Fact
                        label="已处理"
                        value={job.progress.processedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="新增"
                        value={job.progress.insertedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="去重"
                        value={job.progress.deduplicatedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="待归集"
                        value={job.progress.unattributedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="失败"
                        value={job.progress.failedCount.toLocaleString("zh-CN")}
                    />
                </CardContent>
            </Card>
        </div>
    )
}

function ItemsTable({
    items,
    section,
    loading,
    totalCount,
    page,
    onPageChange,
    onReattribute,
    title,
}: {
    items: HistoryBackfillItemView[]
    section: JobSection
    loading?: boolean
    totalCount: number
    page: number
    onPageChange: (page: number) => void
    onReattribute?: (itemId: string) => void
    title?: string
}) {
    const pageSize = 20
    const columns = React.useMemo<ColumnDef<HistoryBackfillItemView>[]>(
        () => [
            {
                id: "factType",
                header: "记录类型",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {FACT_TYPE_LABEL[row.original.factType]}
                    </span>
                ),
            },
            {
                id: "key",
                header: "记录摘要",
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.businessFactKeySummary}
                    </span>
                ),
            },
            {
                id: "order",
                header: "商城订单",
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <div className="font-mono text-xs">
                            {row.original.mallOrderNo}
                        </div>
                        {row.original.sourceDocNo ? (
                            <div className="text-tiny text-muted-foreground">
                                子单 {row.original.sourceDocNo}
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "occurred",
                header: "发生时间",
                cell: ({ row }) => (
                    <span className="num text-xs">
                        {formatDateTime(row.original.occurredAt, "dateStyle")}
                    </span>
                ),
            },
            {
                id: "result",
                header: "结果",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={ITEM_RESULT_LABEL[row.original.result]}
                        tone={ITEM_RESULT_TONE[row.original.result]}
                    />
                ),
            },
            {
                id: "cost",
                header: "成本",
                cell: ({ row }) => {
                    const b = row.original.costBasis
                    if (!b || b === "N_A")
                        return <span className="text-xs">不适用</span>
                    return (
                        <span className="text-xs">
                            {COST_BASIS_LABEL[b]}
                            {b === "NONE"
                                ? " · 成本空"
                                : row.original.costAmountNet
                                  ? ` · ${row.original.costAmountNet}`
                                  : ""}
                        </span>
                    )
                },
            },
            {
                id: "extra",
                header: section === "dedupe" ? "去重证明" : "说明 / 去向",
                cell: ({ row }) => {
                    const item = row.original
                    if (item.dedupeProof) {
                        return (
                            <div className="max-w-[16rem] text-xs">
                                <div>
                                    {item.dedupeProof.matchedSource ===
                                    "REALTIME"
                                        ? "命中实时记录"
                                        : "命中原回填记录"}
                                </div>
                                <div className="text-muted-foreground">
                                    {item.dedupeProof.formalFactSummary}
                                </div>
                            </div>
                        )
                    }
                    if (item.result === "UNATTRIBUTED") {
                        return (
                            <div className="space-y-1">
                                <div className="text-xs">
                                    {item.unattributedReason}
                                </div>
                                <div className="flex flex-wrap gap-1">
                                    <Button
                                        render={
                                            <Link href="/governance/integration-errors?view=mine" />
                                        }
                                        size="sm"
                                        variant="outline"
                                        className="h-7 text-xs"
                                    >
                                        去接口错误中心处理
                                        <ExternalLinkIcon className="size-3" />
                                    </Button>
                                    {onReattribute ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            className="h-7 text-xs"
                                            onClick={() =>
                                                onReattribute(item.itemId)
                                            }
                                        >
                                            重新归集
                                        </Button>
                                    ) : null}
                                </div>
                            </div>
                        )
                    }
                    if (item.failure) {
                        return (
                            <div className="max-w-[14rem] text-xs">
                                <div>
                                    {FAILURE_CODE_LABEL[
                                        item.failure.errorCode
                                    ] ?? item.failure.summary}
                                </div>
                                <div>{item.failure.summary}</div>
                                <div className="text-muted-foreground">
                                    {PIPELINE_STAGE_LABEL[
                                        item.failure
                                            .stage as BackfillPipelineStage
                                    ] ?? item.failure.stage}{" "}
                                    ·{" "}
                                    {item.failure.retryable
                                        ? "可续跑"
                                        : "需业务修复"}
                                </div>
                            </div>
                        )
                    }
                    return (
                        <span className="text-xs text-muted-foreground">
                            {item.fulfillmentChain === "LEGACY_MANUAL"
                                ? "历史手工口径"
                                : "—"}
                        </span>
                    )
                },
            },
        ],
        [section, onReattribute],
    )

    if (items.length === 0) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="当前筛选无明细"
                description="同一商城订单的多笔关键记录分别保留；支付/取消/完成/多次退款/多次余额恢复不会被合并。"
            />
        )
    }

    return (
        <BusinessTableFrame
            title={
                title ??
                (section === "dedupe"
                    ? "去重证明"
                    : section === "unattributed"
                      ? "待归集（原记录已保存）"
                      : section === "failures"
                        ? "失败诊断"
                        : "记录结果")
            }
            description="不含卡号/卡密/手机/完整地址/原始消息内容"
            table={
                <DataTable
                    data={[...items]}
                    columns={columns}
                    getRowId={(row) => row.itemId}
                    rowCount={totalCount}
                    pagination={{ pageIndex: Math.max(0, page - 1), pageSize }}
                    onPaginationChange={(next) =>
                        onPageChange(next.pageIndex + 1)
                    }
                    layout="flush"
                    density="compact"
                    loading={loading}
                    showRefreshingBanner={false}
                />
            }
        />
    )
}

function CostSection({
    job,
    items,
}: {
    job: NonNullable<
        Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
    >["job"]
    items: HistoryBackfillItemView[]
}) {
    return (
        <div className="space-y-4">
            <div className="grid gap-3 md:grid-cols-3">
                {job.costBasis.map((row) => (
                    <Card key={row.basis} className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30 pb-2">
                            <CardTitle className="text-base">
                                {COST_BASIS_LABEL[row.basis]}
                            </CardTitle>
                            <CardDescription>
                                {row.count.toLocaleString("zh-CN")} 笔
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-1 text-sm">
                            <div>
                                消费金额（含税）：{row.consumptionAmountGross}
                            </div>
                            <div>
                                成本净额：
                                {row.basis === "NONE"
                                    ? "空（禁止写 0）"
                                    : (row.costAmountNet ?? "—")}
                            </div>
                        </CardContent>
                    </Card>
                ))}
            </div>
            <Alert>
                <AlertTitle>禁止当前供给价</AlertTitle>
                <AlertDescription>
                    时点标准成本必须命中消费发生时点有效供给版本；未覆盖
                    不得用当前价、猜测税率或销项税率替代进项。覆盖率{" "}
                    {job.coverageRate ?? "—"}（未覆盖进分母）。
                </AlertDescription>
            </Alert>
            <Separator />
            <ItemsTable
                items={items.filter(
                    (i) =>
                        i.costBasis === "ACTUAL" ||
                        i.costBasis === "STANDARD" ||
                        i.costBasis === "NONE",
                )}
                section="facts"
                title="成本口径明细"
                totalCount={
                    items.filter(
                        (i) =>
                            i.costBasis === "ACTUAL" ||
                            i.costBasis === "STANDARD" ||
                            i.costBasis === "NONE",
                    ).length
                }
                page={1}
                onPageChange={() => undefined}
            />
        </div>
    )
}

function ReportSection({
    job,
    report,
    onDownload,
}: {
    job: NonNullable<
        Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
    >["job"]
    report?: NonNullable<
        Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
    >["report"]
    onDownload: () => void
}) {
    if (!report) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="技术报告尚未生成"
                description="处理状态达到部分完成或技术处理完成后可生成可审计报告。"
                className="rounded-lg border-0 bg-transparent shadow-none ring-0"
            />
        )
    }

    const unconfirmed = report.reviewLabel === "UNCONFIRMED"

    return (
        <div className="space-y-4">
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                        <div>
                            <CardTitle>审计报告</CardTitle>
                            <CardDescription>
                                v{report.reportVersion} ·{" "}
                                {formatDateTime(
                                    report.generatedAt,
                                    "dateStyle",
                                )}
                            </CardDescription>
                        </div>
                        <div className="flex flex-wrap gap-2">
                            <Badge
                                variant={unconfirmed ? "outline" : "default"}
                            >
                                {report.downloadLabel}
                            </Badge>
                            <BusinessStatusBadge
                                context="detail"
                                label={
                                    PROCESSING_STATUS_LABEL[
                                        report.processingStatus
                                    ]
                                }
                                tone={
                                    PROCESSING_STATUS_TONE[
                                        report.processingStatus
                                    ]
                                }
                            />
                            <BusinessStatusBadge
                                context="detail"
                                label={
                                    REPORT_REVIEW_STATUS_LABEL[
                                        report.reportReviewStatus
                                    ]
                                }
                                tone={
                                    REPORT_REVIEW_STATUS_TONE[
                                        report.reportReviewStatus
                                    ]
                                }
                            />
                        </div>
                    </div>
                </CardHeader>
                <CardContent className="space-y-4">
                    {unconfirmed ? (
                        <Alert>
                            <TriangleAlertIcon />
                            <AlertTitle>技术报告 · 未确认</AlertTitle>
                            <AlertDescription>
                                报告复核策略未配置或报告未确认时，下载固定标「技术报告
                                ·
                                未确认」。确认动作与下游门禁保持关闭；不得仅因技术完成解锁。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {report.fullHistoryFinalComplete ? (
                        <Alert>
                            <AlertTitle>全历史回填最终完成</AlertTitle>
                            <AlertDescription>
                                技术处理完成、来源覆盖完整且报告已确认。
                            </AlertDescription>
                        </Alert>
                    ) : (
                        <Alert>
                            <AlertTitle>尚未全历史最终完成</AlertTitle>
                            <AlertDescription>
                                当前不可宣称「全历史回填最终完成」。后续流程：
                                {job.formalDownstreamUnlocked
                                    ? "已可用"
                                    : "保持关闭"}
                                。
                            </AlertDescription>
                        </Alert>
                    )}

                    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                        <Fact
                            label="范围"
                            value={`${formatDay(report.rangeStart)} 至 ${formatDay(report.rangeEnd)}（截止时点当天除外）`}
                            mono
                        />
                        <Fact
                            label="启用日"
                            value={formatDay(report.cutoverAt)}
                            mono
                        />
                        <Fact
                            label="总笔数"
                            value={report.totalCount.toLocaleString("zh-CN")}
                        />
                        <Fact label="总金额" value={report.totalAmount} />
                        <Fact
                            label="去重"
                            value={report.deduplicatedCount.toLocaleString(
                                "zh-CN",
                            )}
                        />
                        <Fact
                            label="覆盖率"
                            value={report.coverageRate ?? "—"}
                        />
                        <Fact
                            label="报告格式"
                            value={report.schemaVersion}
                            mono
                        />
                        <Fact
                            label="规则版本"
                            value={report.ruleVersion}
                            mono
                        />
                        <Fact label="操作者" value={report.operatorLabel} />
                    </div>

                    <div className="grid gap-3 md:grid-cols-3">
                        {report.costBasis.map((c) => (
                            <div
                                key={c.basis}
                                className="rounded-xl border bg-muted/30 p-3 text-sm"
                            >
                                <div className="font-medium">
                                    {COST_BASIS_LABEL[c.basis]}
                                </div>
                                <div>{c.count.toLocaleString("zh-CN")} 笔</div>
                                <div>{c.consumptionAmountGross}</div>
                                <div className="text-muted-foreground">
                                    成本：
                                    {c.basis === "NONE"
                                        ? "空"
                                        : (c.costAmountNet ?? "—")}
                                </div>
                            </div>
                        ))}
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                        <div>
                            <h4 className="mb-2 text-sm font-medium">
                                未归集清单摘要
                            </h4>
                            <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                                {report.unattributedSummaries.map((s) => (
                                    <li key={s}>{s}</li>
                                ))}
                            </ul>
                        </div>
                        <div>
                            <h4 className="mb-2 text-sm font-medium">
                                失败清单摘要
                            </h4>
                            <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                                {report.failedSummaries.map((s) => (
                                    <li key={s}>{s}</li>
                                ))}
                            </ul>
                        </div>
                    </div>

                    <p className="text-xs text-muted-foreground">
                        {report.sensitiveRedactionNote}
                    </p>

                    <div className="flex flex-wrap gap-2">
                        <Button type="button" onClick={onDownload}>
                            <DownloadIcon className="size-4" />
                            下载
                            {unconfirmed ? "技术报告 · 未确认" : "已确认报告"}
                            （示例）
                        </Button>
                        {job.reportReviewStatus === "POLICY_NOT_CONFIGURED" ? (
                            <Badge variant="outline">
                                确认动作不可用 · 复核策略未配置
                            </Badge>
                        ) : null}
                        {job.allowedActions.includes("CONFIRM_REPORT") ? (
                            <Badge variant="outline">
                                报告确认后解锁后续流程
                            </Badge>
                        ) : null}
                    </div>
                </CardContent>
            </Card>
        </div>
    )
}

export { CostSection, ItemFilters, ItemsTable, OverviewSection, ReportSection }
