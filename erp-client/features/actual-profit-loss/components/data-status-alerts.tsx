import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { SystemFeedback } from "@/components/business/system-feedback"
import type {
    ProfitLossExport,
    ProfitLossView,
} from "@/features/actual-profit-loss/types"

export function DataStatusAlerts({
    data,
    refreshFailed,
    exportJob,
    onCloseExportJob,
}: {
    data: ProfitLossView
    refreshFailed: string | null
    exportJob: ProfitLossExport | null
    onCloseExportJob: () => void
}) {
    return (
        <>
            <SystemFeedback
                problemId="profit-loss-background-refresh"
                error={
                    data.freshness.state === "failed"
                        ? "数据更新失败，请联系管理员排查后台任务。"
                        : null
                }
                title="经营数据尚未更新"
                description="已保留上次成功结果，业务记录未被修改。请稍后刷新或联系管理员。"
            />
            {data.freshness.state === "stale" || refreshFailed ? (
                <p role="status" className="text-sm text-muted-foreground">
                    当前显示上次成功结果，更新于{" "}
                    {formatDateTime(data.freshness.projectedAt, "full")}。
                </p>
            ) : null}
            {data.freshness.state === "rebuilding" ? (
                <p role="status" className="text-sm text-muted-foreground">
                    数据更新中，已保留上次成功结果；导出将标注数据时间。
                </p>
            ) : null}
            {data.freshness.state === "failed" ? (
                <p role="status" className="text-sm text-muted-foreground">
                    当前显示上次成功结果，最新数据暂不可用。
                </p>
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
                <Alert>
                    <AlertTitle>导出完成</AlertTitle>
                    <AlertDescription>
                        已导出全部匹配结果 {exportJob.rowCount} 行，生成于{" "}
                        {formatDateTime(exportJob.generatedAt, "full")}。
                        导出使用生成时的最新数据与权限，可能与刷新前的页面结果不同。
                        <Button
                            id="actual-profit-loss-export-close"
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onCloseExportJob}
                        >
                            关闭
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
