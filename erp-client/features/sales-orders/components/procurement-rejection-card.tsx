"use client"

import type { ReactNode } from "react"

import { DocumentSection, MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { PROCUREMENT_REJECT_REASON_LABEL } from "@/features/sales-orders/labels"
import type {
    ProcurementRejectionResolution,
    SalesOrderListItem,
} from "@/features/sales-orders/types"

type ProcurementRejectionCardProps = {
    order: SalesOrderListItem
    rejection: ProcurementRejectionResolution
    canAct: boolean
}

/**
 * 采购驳回事实区：只展示原因和差异。改完再报 / 作废放在对象头，不在这里重复。
 */
export function ProcurementRejectionCard({
    order,
    rejection,
    canAct,
}: ProcurementRejectionCardProps) {
    const resolved =
        rejection.reviewStatus === "RESOLVED" ||
        rejection.reviewStatus === "VOIDED" ||
        Boolean(rejection.resolutionOutcome)
    const reasonLabel =
        PROCUREMENT_REJECT_REASON_LABEL[rejection.rejectReasonCode] ??
        rejection.rejectReasonCode
    const hasDiff = rejection.draftDifference.diffSummary.length > 0
    const changedCommercial =
        rejection.draftDifference.changedItemOrService ||
        rejection.draftDifference.changedSalesPrice

    return (
        <DocumentSection title="采购驳回">
            <div className="space-y-4">
                {rejection.resolutionOutcome ? (
                    <Alert
                        variant={
                            rejection.resolutionOutcome.outcome.includes("VOID")
                                ? "destructive"
                                : "success"
                        }
                    >
                        <AlertTitle>处理结果</AlertTitle>
                        <AlertDescription>
                            {rejection.resolutionOutcome.detail}
                            {rejection.resolutionOutcome.newWorkItemId
                                ? " · 已生成后续待办，请相关同事继续处理。"
                                : null}
                        </AlertDescription>
                    </Alert>
                ) : null}

                <dl className="grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2">
                    <Fact label="原因" value={reasonLabel} />
                    <Fact
                        label="谁驳回 / 何时"
                        value={`${rejection.rejectedByLabel} · ${rejection.rejectedAt}`}
                    />
                    <Fact
                        label="说明"
                        value={rejection.rejectComment || "—"}
                        className="sm:col-span-2"
                    />
                    {rejection.estimatedCost ? (
                        <Fact
                            label="采购最新成本"
                            value={
                                <MoneyValue
                                    value={rejection.estimatedCost}
                                    taxBasis="gross"
                                />
                            }
                        />
                    ) : null}
                    {rejection.estimatedMarginPercent ? (
                        <Fact
                            label="预计毛利"
                            value={`${rejection.estimatedMarginPercent}%`}
                            numeric
                        />
                    ) : null}
                    <Fact
                        label="报给采购的次数"
                        value={`第 ${rejection.rejectedSubmissionNo} 次`}
                        numeric
                    />
                </dl>

                <section aria-label="相对被驳回内容的变化">
                    <h3 className="text-sm font-semibold">
                        相对被驳回内容的变化
                    </h3>
                    {hasDiff ? (
                        <ul className="mt-2 space-y-1.5 text-sm">
                            {rejection.draftDifference.diffSummary.map(
                                (item) => (
                                    <li
                                        key={item.field}
                                        className="rounded-md border border-border px-3 py-2"
                                    >
                                        <span className="font-medium">
                                            {item.field}
                                        </span>
                                        <span className="mt-0.5 block text-xs text-muted-foreground">
                                            {item.before} → {item.after}
                                        </span>
                                    </li>
                                ),
                            )}
                        </ul>
                    ) : (
                        <p className="mt-2 text-sm text-muted-foreground">
                            {changedCommercial
                                ? "商品或价格已有改动，可用页头「改完再报」核对整单。"
                                : "还没改商品或价格。改完后才能再报给采购。"}
                        </p>
                    )}
                    <p className="mt-2 text-xs text-muted-foreground">
                        商品/价格：
                        {changedCommercial ? "已有改动" : "还没改"}
                        {" · "}
                        与原报采购内容是否一致：
                        {rejection.draftDifference.commercialTermsUnchanged
                            ? "一致"
                            : "已不一致"}
                    </p>
                </section>

                {!resolved && !canAct ? (
                    <p className="text-xs text-muted-foreground">
                        当前账号不能改这张单，也不能作废。销售单{" "}
                        {order.documentNumber} 仍停在采购驳回后的待处理。
                    </p>
                ) : null}
            </div>
        </DocumentSection>
    )
}

function Fact({
    label,
    value,
    numeric,
    className,
}: {
    label: string
    value: ReactNode
    numeric?: boolean
    className?: string
}) {
    return (
        <div className={`bg-background px-3 py-2 ${className ?? ""}`}>
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd
                className={`mt-0.5 text-sm font-medium ${numeric ? "num" : ""}`}
            >
                {value}
            </dd>
        </div>
    )
}
