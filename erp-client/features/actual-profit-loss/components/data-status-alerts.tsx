import {
    BackgroundJobProgress,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import type {
    ProfitLossExportJob,
    ProfitLossView,
} from "@/features/actual-profit-loss/types"
import { basisLabel } from "@/features/actual-profit-loss/lib/url-state"

export function DataStatusAlerts({
    data,
    refreshFailed,
    exportFailed,
    viewError,
    isViewError,
    exportJob,
    onCloseExportJob,
}: {
    data: ProfitLossView
    refreshFailed: string | null
    exportFailed: string | null
    viewError: unknown
    isViewError: boolean
    exportJob: ProfitLossExportJob | null
    onCloseExportJob: () => void
}) {
    return (
        <>
            {data.freshness.state === "stale" || refreshFailed ? (
                <Alert variant="warning">
                    <AlertTitle>
                        {refreshFailed ? "刷新失败" : "数据陈旧"}
                    </AlertTitle>
                    <AlertDescription>
                        {refreshFailed
                            ? refreshFailed
                            : `数据更新于 ${formatDateTime(data.freshness.projectedAt, "full")}，来源已于 ${formatDateTime(data.freshness.sourceWatermark, "full")} 更新。`}
                    </AlertDescription>
                </Alert>
            ) : null}

            {data.freshness.state === "rebuilding" ? (
                <Alert>
                    <AlertTitle>数据更新中</AlertTitle>
                    <AlertDescription>
                        更新中，已保留上次成功结果；导出将标注旧数据时间。
                    </AlertDescription>
                </Alert>
            ) : null}

            {data.freshness.state === "failed" ? (
                <Alert variant="destructive">
                    <AlertTitle>数据更新失败</AlertTitle>
                    <AlertDescription>
                        经营记录未被修改；展示上次成功数据并标记失败。请联系管理员排查后台任务。
                    </AlertDescription>
                </Alert>
            ) : null}

            {isViewError ? (
                <Alert variant="destructive">
                    <AlertTitle>数据更新失败</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(
                            viewError,
                            "已保留上次成功结果，未覆盖业务数据。请重试或调整筛选。",
                        )}
                    </AlertDescription>
                </Alert>
            ) : null}

            {exportFailed ? (
                <Alert variant="destructive">
                    <AlertTitle>导出失败</AlertTitle>
                    <AlertDescription>{exportFailed}</AlertDescription>
                </Alert>
            ) : null}

            {data.correctionPendingNotice ? (
                <Alert>
                    <AlertTitle>来源纠错已登记</AlertTitle>
                    <AlertDescription>
                        {data.correctionPendingNotice}
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
                    label="实际经营盈亏导出"
                    description={
                        <>
                            期间 {exportJob.watermark.periodFrom}~
                            {exportJob.watermark.periodTo} · 归属口径{" "}
                            {basisLabel(exportJob.watermark.periodBasis)} ·
                            数据更新于 {exportJob.watermark.projectedAt}
                            {exportJob.downloadLabel ? (
                                <span className="mt-1 block font-medium">
                                    可下载：{exportJob.downloadLabel}
                                </span>
                            ) : null}
                        </>
                    }
                    action={
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => onCloseExportJob()}
                        >
                            关闭
                        </Button>
                    }
                />
            ) : null}
        </>
    )
}
