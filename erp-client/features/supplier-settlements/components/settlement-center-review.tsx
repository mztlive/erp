"use client"

import { DetailWorkspaceSection } from "@/components/business/detail-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { REASON_CODE_LABEL } from "@/features/supplier-settlements/types"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"

function SettlementCenterReview({
    detail,
    embedded = false,
}: {
    detail: SettlementDetailView
    embedded?: boolean
}) {
    return (
        <DetailWorkspaceSection title="复核记录" width="form">
            {!embedded && detail.workItem ? (
                <Alert variant="info">
                    <AlertTitle>复核任务</AlertTitle>
                    <AlertDescription>
                        <span className="num">
                            {detail.statement.statementNo}
                        </span>{" "}
                        · 供应商 {detail.statement.supplierName}
                        {detail.workItem.ownerUser
                            ? ` · 当前处理人 ${detail.workItem.ownerUser.displayName}`
                            : " · 当前责任人信息不可用"}
                    </AlertDescription>
                </Alert>
            ) : null}
            {detail.reviewRecords.length === 0 ? (
                <p className="text-xs text-muted-foreground">尚无复核记录</p>
            ) : (
                detail.reviewRecords.map((r) => (
                    <div
                        key={r.recordId}
                        className="border-t border-border/60 py-2 text-body-compact first:border-t-0 first:pt-0"
                    >
                        <div className="font-medium">
                            {r.actionLabel} · {r.by.displayName}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            <span className="num">
                                {formatDateTime(r.at, "default")}
                            </span>
                            {r.reasonCode
                                ? ` · ${REASON_CODE_LABEL[r.reasonCode] ?? r.reasonCode}`
                                : ""}
                            {r.comment ? ` · ${r.comment}` : ""}
                        </div>
                    </div>
                ))
            )}
        </DetailWorkspaceSection>
    )
}

export { SettlementCenterReview }
