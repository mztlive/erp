"use client"

import {
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
} from "@/components/business"
import { Textarea } from "@/components/ui/textarea"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceDialogs({
    confirmOpen,
    onConfirmOpenChange,
    overallPreview,
    hasExceptionResult,
    onConfirmAcceptance,
    reverseTarget,
    onReverseOpenChange,
    reverseReason,
    onReverseReasonChange,
    onConfirmReverse,
    exitDiscardOpen,
    onExitDiscardOpenChange,
    onConfirmExit,
}: {
    confirmOpen: boolean
    onConfirmOpenChange: (open: boolean) => void
    overallPreview: AcceptanceOverallResult
    hasExceptionResult: boolean
    onConfirmAcceptance: () => Promise<void>
    reverseTarget: AcceptanceHistoryItem | null
    onReverseOpenChange: (open: boolean) => void
    reverseReason: string
    onReverseReasonChange: (value: string) => void
    onConfirmReverse: () => Promise<void>
    exitDiscardOpen: boolean
    onExitDiscardOpenChange: (open: boolean) => void
    onConfirmExit: () => void
}) {
    return (
        <>
            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={onConfirmOpenChange}
                title="确认客户验收"
                actionLabel="确认本次验收"
                confirmLabel="确认本次验收"
                fromStatus={{ label: "待登记", tone: "warning" }}
                toStatus={{
                    label: OVERALL_RESULT_LABEL[overallPreview],
                    tone:
                        overallPreview === "PASS"
                            ? "success"
                            : overallPreview === "SHORT"
                              ? "warning"
                              : "destructive",
                }}
                lockedFields={["交付数量以当前记录为准"]}
                effects={[
                    "记下本次客户验收结果",
                    "按通过数量推进本单交付进度",
                    ...(hasExceptionResult
                        ? ["短少或拒收不会自动退货、退款或改应收"]
                        : []),
                ]}
                nextDepartment={
                    hasExceptionResult ? "退货或拒收处理" : "销售与财务"
                }
                onConfirm={onConfirmAcceptance}
            />

            <FormalActionConfirmDialog
                open={Boolean(reverseTarget)}
                onOpenChange={onReverseOpenChange}
                title="确认冲正误录验收"
                actionLabel="冲正"
                confirmLabel="确认冲正"
                fromStatus={{ label: "已确认", tone: "success" }}
                toStatus={{ label: "已冲正（新增反向记录）", tone: "warning" }}
                lockedFields={["原验收单号"]}
                effects={[
                    "新增反向验收记录",
                    "恢复对应批次的待验数量",
                    "不删除原验收记录",
                ]}
                nextDepartment="销售"
                description={
                    <div className="space-y-2">
                        <span>请填写冲正理由，便于以后核对：</span>
                        <Textarea
                            aria-label="冲正理由"
                            rows={3}
                            value={reverseReason}
                            onChange={(event) =>
                                onReverseReasonChange(event.target.value)
                            }
                            placeholder="说明误录原因"
                        />
                    </div>
                }
                onConfirm={onConfirmReverse}
            />

            <DiscardConfirmDialog
                open={exitDiscardOpen}
                onOpenChange={onExitDiscardOpenChange}
                title="放弃本次验收登记？"
                description="已勾选的批次和填写的结果还没提交，取消后会丢掉。"
                confirmLabel="放弃并返回"
                cancelLabel="继续登记"
                onConfirm={onConfirmExit}
            />
        </>
    )
}
