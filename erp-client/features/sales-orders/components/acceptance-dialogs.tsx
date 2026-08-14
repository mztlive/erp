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
                actionLabel="确认验收"
                confirmLabel="确认验收"
                fromStatus={{ label: "草稿", tone: "warning" }}
                toStatus={{
                    label: OVERALL_RESULT_LABEL[overallPreview],
                    tone:
                        overallPreview === "PASS"
                            ? "success"
                            : overallPreview === "SHORT"
                              ? "warning"
                              : "destructive",
                }}
                lockedFields={[
                    "履约记录版本",
                    "净可验收量（系统）",
                    "销售单数据版本",
                ]}
                effects={[
                    "生成客户验收记录",
                    "按本次结果分配履约数量",
                    "更新销售履约数据",
                    ...(hasExceptionResult
                        ? ["不扣库存、不改应收、不自动退货（仅验收记录）"]
                        : []),
                ]}
                nextDepartment={
                    hasExceptionResult
                        ? "变更与异常 / 销售协同"
                        : "销售与财务协同"
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
                lockedFields={["原验收单号", "原分配明细"]}
                effects={[
                    "新增反向验收记录",
                    "写入反向分配（不修改原记录）",
                    "恢复对应履约批次净可验收量",
                ]}
                nextDepartment="销售"
                description={
                    <div className="space-y-2">
                        <span>
                            冲正将新增反向记录，不会删除或改写原验收行。请填写冲正理由：
                        </span>
                        <Textarea
                            aria-label="冲正理由"
                            rows={3}
                            value={reverseReason}
                            onChange={(e) =>
                                onReverseReasonChange(e.target.value)
                            }
                            placeholder="说明误录原因，供后续追溯"
                        />
                    </div>
                }
                onConfirm={onConfirmReverse}
            />

            <DiscardConfirmDialog
                open={exitDiscardOpen}
                onOpenChange={onExitDiscardOpenChange}
                title="放弃本次验收登记？"
                description="已录入的分配数量与结果尚未保存为草稿，退出后将丢失。"
                confirmLabel="放弃并退出"
                cancelLabel="继续登记"
                onConfirm={onConfirmExit}
            />
        </>
    )
}
