"use client"

import { surfaceInsetClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { REASON_CODE_LABEL } from "@/features/supplier-settlements/types"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

function SettlementCenterReview({
    detail,
}: {
    detail: SettlementDetailView
}) {
    return (
        <Card
            size="sm"
            className={cn(surfaceInsetClassName, "shadow-none ring-0")}
        >
            <CardHeader className="rounded-t-lg border-b border-grid py-3">
                <CardTitle className="text-base">复核记录</CardTitle>
                <CardDescription>
                    提交 / 驳回 / 确认追加式记录；岗位分离由系统校验
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {detail.workItem ? (
                    <Alert variant="info">
                        <AlertTitle>复核任务</AlertTitle>
                        <AlertDescription>
                            {detail.statement.statementNo} · 供应商{" "}
                            {detail.statement.supplierName}
                            {detail.workItem.ownerUser
                                ? ` · 当前处理人 ${detail.workItem.ownerUser.displayName}`
                                : detail.workItem.assignmentMode === "POOL"
                                  ? " · 团队池待开始处理"
                                  : " · 尚无个人责任人"}
                        </AlertDescription>
                    </Alert>
                ) : null}
                {detail.reviewRecords.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚无复核记录
                    </p>
                ) : (
                    detail.reviewRecords.map((r) => (
                        <div
                            key={r.recordId}
                            className={cn(
                                surfaceInsetClassName,
                                "px-3 py-2 text-sm",
                            )}
                        >
                            <div className="font-medium">
                                {r.actionLabel} · {r.by.displayName}
                            </div>
                            <div className="text-xs text-muted-foreground">
                                {formatDateTime(r.at, "default")}
                                {r.reasonCode
                                    ? ` · ${REASON_CODE_LABEL[r.reasonCode] ?? r.reasonCode}`
                                    : ""}
                                {r.comment ? ` · ${r.comment}` : ""}
                            </div>
                        </div>
                    ))
                )}
            </CardContent>
        </Card>
    )
}

export { SettlementCenterReview }
