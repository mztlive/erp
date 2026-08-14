import { QuickPreviewSheet } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import type { CardBusinessAnalyticsView } from "../types"

export interface CardBusinessExportSheetProps {
    open: boolean
    onOpenChange: (open: boolean) => void
    data: CardBusinessAnalyticsView | undefined
    isExporting: boolean
    onConfirmExport: () => void
}

/** 导出预览：口径/筛选/水位/覆盖率 disclaimer。 */
export function CardBusinessExportSheet({
    open,
    onOpenChange,
    data,
    isExporting,
    onConfirmExport,
}: CardBusinessExportSheetProps) {
    return (
        <QuickPreviewSheet
            open={open}
            onOpenChange={onOpenChange}
            title="导出预览"
            description="导出为当前查询数据记录，非台账副本；下载时重新鉴权。"
        >
            {data ? (
                <div className="space-y-4 text-sm">
                    <Alert variant="warning">
                        <AlertTitle>导出免责声明</AlertTitle>
                        <AlertDescription className="space-y-2 text-xs">
                            <p>
                                <strong>口径：</strong>
                                销售/面值/消费/余额为含税；成本/毛差/经营贡献为不含税。无成本数据不计入利润。
                            </p>
                            <p>
                                <strong>筛选：</strong>
                                {data.filterSummary}
                            </p>
                            <p>
                                <strong>数据时间：</strong>
                                数据{" "}
                                {formatDateTime(
                                    data.freshness.projectionUpdatedAt,
                                    "full",
                                )}{" "}
                                · 同步{" "}
                                {formatDateTime(
                                    data.freshness.consumedOutboxWatermark,
                                    "full",
                                )}{" "}
                                · 余额记录{" "}
                                {formatDateTime(
                                    data.freshness.balanceSnapshotAt,
                                    "full",
                                )}{" "}
                                · 延迟 {data.freshness.lagSeconds}s / 上限{" "}
                                {data.freshness.maxLagSeconds}s
                            </p>
                            <p>
                                <strong>覆盖率：</strong>
                                {data.coverage.rate ?? "—"}（阈值{" "}
                                {data.coverage.threshold}）
                                {data.coverage.profitReferenceOnly
                                    ? " · 成本不完整，结果仅供参考"
                                    : ""}
                            </p>
                            <p>
                                <strong>微信排除：</strong>
                                {data.wechatExcludedNote}
                            </p>
                            <p>
                                <strong>数据范围：</strong>
                                行数 {data.rows.total}
                                ；明细表已按当前筛选过滤并按分析视角聚合，指标与图表为全量口径。
                            </p>
                        </AlertDescription>
                    </Alert>
                    <div className="flex justify-end gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={isExporting}
                            onClick={onConfirmExport}
                        >
                            确认导出
                        </Button>
                    </div>
                </div>
            ) : null}
        </QuickPreviewSheet>
    )
}
