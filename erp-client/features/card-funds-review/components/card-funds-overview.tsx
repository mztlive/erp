import { TriangleAlertIcon } from "lucide-react"

import {
    BusinessDiffPanel,
    DocumentSummary,
    MetricItem,
    MetricStrip,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { CardFundsReviewItemView } from "../types"
import { formatMoney, shortHash } from "../presentation"
import { versionText } from "@/lib/ui-text"

export function CardFundsOverview({ task }: { task: CardFundsReviewItemView }) {
    return (
        <>
            <DocumentSummary
                columns="two"
                items={[
                    {
                        id: "order",
                        label: "卡券销售单",
                        value: task.salesOrder.orderNo,
                        emphasized: true,
                    },
                    {
                        id: "hash",
                        label: "当前数据版本",
                        value: (
                            <span className="num font-mono text-sm">
                                {shortHash(task.workItem.subjectHash)}
                            </span>
                        ),
                        description: task.workItem.subjectHash,
                    },
                    {
                        id: "counterparty",
                        label: "收款/开票往来主体",
                        value: task.account.counterpartyPartyName,
                    },
                    {
                        id: "reason",
                        label: "任务原因",
                        value: task.workItem.reason,
                    },
                ]}
            />

            <MetricStrip columns={5} aria-label="票款记录指标">
                <MetricItem
                    label="同步成交额"
                    value={formatMoney(task.account.syncedGrossAmount)}
                    detail="商城当前版本"
                />
                <MetricItem
                    label="当前应收"
                    value={formatMoney(task.account.grossTotal)}
                    detail={`开放 ${formatMoney(task.account.openTotal)}`}
                />
                <MetricItem
                    label="净已收"
                    value={formatMoney(task.account.settledTotal)}
                    detail="净额（已收减冲正）"
                />
                <MetricItem
                    label="净已开票"
                    value={formatMoney(task.account.invoicedTotal)}
                    detail={`可开 ${formatMoney(task.account.openInvoiceableTotal)}`}
                />
                <MetricItem
                    label={versionText.versionStatus}
                    value={task.fingerprintStatus.label}
                    detail={task.fingerprintStatus.detail}
                    status={{
                        label: task.fingerprintStatus.label,
                        tone: task.fingerprintStatus.tone,
                    }}
                />
            </MetricStrip>

            <Alert
                variant={
                    task.account.fundsReliability === "VERIFIED"
                        ? "default"
                        : "destructive"
                }
            >
                <TriangleAlertIcon aria-hidden="true" />
                <AlertTitle>
                    {task.account.fundsReliability ===
                    "UNRELIABLE_PENDING_REVIEW"
                        ? "票款指标不可靠（复核未完成）"
                        : task.account.fundsReliability === "STALE_FINGERPRINT"
                          ? "数据已变更 · 指标不可靠"
                          : "可靠性"}
                </AlertTitle>
                <AlertDescription>
                    {task.account.reliabilityNote}
                    复核未完成前，指标不可视为已核实。
                </AlertDescription>
            </Alert>

            {task.reviewType === "SYNC_DELTA" && task.difference ? (
                <div className="space-y-2">
                    {(() => {
                        const moneyChanges = task.difference!.changes.filter(
                            (c) =>
                                /成交额|应收|已收|已开票/.test(c.field) &&
                                Number.isFinite(Number(c.before)) &&
                                Number.isFinite(Number(c.after)),
                        )
                        if (moneyChanges.length === 0) return null
                        const totalDelta = moneyChanges.reduce(
                            (s, c) => s + (Number(c.after) - Number(c.before)),
                            0,
                        )
                        return (
                            <p className="text-sm text-muted-foreground">
                                金额类字段合计差额：{" "}
                                <span
                                    className={
                                        totalDelta >= 0
                                            ? "num text-foreground"
                                            : "num text-destructive"
                                    }
                                >
                                    {formatMoney(
                                        Math.abs(totalDelta).toFixed(2),
                                    )}
                                </span>
                                {totalDelta >= 0 ? "（增加）" : "（减少）"}
                            </p>
                        )
                    })()}
                    <BusinessDiffPanel
                        title={task.difference.title}
                        caption="上一有效复核与当前记录对比（系统最新数据）"
                        changes={task.difference.changes.map((c) => ({
                            id: c.id,
                            field: c.field,
                            before: c.before,
                            after: c.after,
                            note: [c.note, c.sourceObject, c.occurredAt]
                                .filter(Boolean)
                                .join(" · "),
                        }))}
                    />
                </div>
            ) : null}
        </>
    )
}
