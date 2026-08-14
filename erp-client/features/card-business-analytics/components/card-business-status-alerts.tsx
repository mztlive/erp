import Link from "next/link"

import { BackgroundJobProgress } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { downloadCardBusinessCsv } from "../lib/export-csv"
import type {
    CardBusinessAnalyticsView,
    CardBusinessExportJob,
} from "../types"

export interface CardBusinessStatusAlertsProps {
    data: CardBusinessAnalyticsView
    viewError: unknown
    refreshFailed: string | null
    exportJob: CardBusinessExportJob | null
    onCloseExportJob: () => void
}

/** 数据存在时的状态提示簇：SLA/重建/失败/查询错误/导出任务进度。 */
export function CardBusinessStatusAlerts({
    data,
    viewError,
    refreshFailed,
    exportJob,
    onCloseExportJob,
}: CardBusinessStatusAlertsProps) {
    return (
        <>
            {data.freshness.slaState === "BREACHED" ||
            data.freshness.state === "stale" ||
            refreshFailed ? (
                <Alert variant="warning">
                    <AlertTitle>
                        {refreshFailed
                            ? "刷新失败"
                            : "数据陈旧 · 更新超时 · 非实时"}
                    </AlertTitle>
                    <AlertDescription>
                        {refreshFailed
                            ? refreshFailed
                            : `更新延迟 ${data.freshness.lagSeconds}s 超过固定上限 ${data.freshness.maxLagSeconds}s（${
                                  data.freshness.slaState === "BREACHED"
                                      ? "已超时"
                                      : data.freshness.slaState === "REBUILDING"
                                        ? "重建中"
                                        : "异常"
                              }）。数据 ${formatDateTime(data.freshness.projectionUpdatedAt, "full")}，同步 ${formatDateTime(data.freshness.consumedOutboxWatermark, "full")}。余额记录独立显示，不合并为「实时」。`}
                    </AlertDescription>
                </Alert>
            ) : null}

            {data.freshness.state === "rebuilding" ? (
                <Alert>
                    <AlertTitle>数据更新中</AlertTitle>
                    <AlertDescription>
                        保留最近成功结果只读查看；导出将标注旧数据时间。更新只修复查询数据，不修改业务记录。
                    </AlertDescription>
                </Alert>
            ) : null}

            {data.freshness.state === "failed" ? (
                <Alert variant="destructive">
                    <AlertTitle>数据更新失败</AlertTitle>
                    <AlertDescription>
                        经营记录未被修改；展示上次成功数据。可前往接口错误中心查看数据异常。
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="ml-2"
                            render={
                                <Link
                                    href={data.governanceLinks.integrationErrorsHref}
                                />
                            }
                        >
                            {openWorkspaceLabel("W29")}
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}

            {viewError ? (
                <Alert variant="destructive">
                    <AlertTitle>数据更新失败</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(
                            viewError,
                            "已保留上次成功结果供只读查阅，未覆盖任何金额。请重试或调整筛选。",
                        )}
                    </AlertDescription>
                </Alert>
            ) : null}

            {exportJob ? (
                <BackgroundJobProgress
                    mode="all-or-nothing"
                    status={
                        exportJob.status === "queued"
                            ? "queued"
                            : exportJob.status === "running"
                              ? "running"
                              : exportJob.status === "succeeded"
                                ? "succeeded"
                                : "failed"
                    }
                    total={exportJob.total}
                    completed={exportJob.completed}
                    succeeded={
                        exportJob.status === "succeeded"
                            ? exportJob.total
                            : undefined
                    }
                    label="卡券经营分析导出"
                    description={
                        <>
                            口径/筛选：
                            {exportJob.watermark.filterSummary}
                            <span className="mt-1 block">
                                覆盖率{" "}
                                {exportJob.watermark.coverageRate ?? "—"} ·
                                数据{" "}
                                {formatDateTime(
                                    exportJob.watermark.projectionUpdatedAt,
                                    "full",
                                )}{" "}
                                · 同步{" "}
                                {formatDateTime(
                                    exportJob.watermark.consumedOutboxWatermark,
                                    "full",
                                )}{" "}
                                · 延迟 {exportJob.watermark.lagSeconds}s
                            </span>
                            <span className="mt-1 block text-xs">
                                {exportJob.watermark.taxDisclaimer}
                            </span>
                            {exportJob.downloadLabel ? (
                                <span className="mt-1 block font-medium">
                                    导出文件：{exportJob.downloadLabel}
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        className="ml-2"
                                        onClick={() =>
                                            downloadCardBusinessCsv(
                                                data,
                                                exportJob,
                                            )
                                        }
                                    >
                                        下载 CSV
                                    </Button>
                                </span>
                            ) : null}
                        </>
                    }
                    action={
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onCloseExportJob}
                        >
                            关闭
                        </Button>
                    }
                />
            ) : null}
        </>
    )
}
