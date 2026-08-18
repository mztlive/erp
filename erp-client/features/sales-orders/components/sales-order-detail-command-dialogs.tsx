"use client"

import * as React from "react"
import { BanIcon, FilePenLineIcon } from "lucide-react"

import { FormalActionConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { VoidSalesOrderDialog } from "@/features/sales-orders/components/void-sales-order-dialog"
import type { ActionBlocker } from "@/features/sales-orders/types"

export function SalesOrderDetailSecondaryActions({
    order,
    openRejection,
    canRequestLowMargin,
    canVoid,
    canStartChange,
    changeBlocker,
    changePending,
    onOpenLowMargin,
    onOpenVoid,
    onOpenChangeConfirm,
}: {
    order: SalesOrderDetailView
    openRejection: boolean
    canRequestLowMargin: boolean
    canVoid: boolean
    canStartChange: boolean
    changeBlocker?: ActionBlocker
    changePending: boolean
    onOpenLowMargin: () => void
    onOpenVoid: () => void
    onOpenChangeConfirm: () => void
}) {
    return (
        <div className="flex flex-wrap items-center gap-2">
            {openRejection && canRequestLowMargin ? (
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={onOpenLowMargin}
                >
                    申请低毛利承接
                </Button>
            ) : null}
            {openRejection && canVoid ? (
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={onOpenVoid}
                >
                    <BanIcon data-icon="inline-start" aria-hidden="true" />
                    作废
                </Button>
            ) : null}
            {!openRejection ? (
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={!canStartChange || changePending}
                    title={
                        !canStartChange
                            ? (changeBlocker?.reason ??
                              order.commercialReadOnlyReason ??
                              "当前不能改单")
                            : undefined
                    }
                    onClick={onOpenChangeConfirm}
                >
                    <FilePenLineIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    发起改单
                </Button>
            ) : null}
        </div>
    )
}

export function SalesOrderDetailCommandDialogs({
    order,
    isCard,
    voidOpen,
    onVoidOpenChange,
    voidPending,
    onVoidConfirm,
    lowMarginOpen,
    onLowMarginOpenChange,
    lowMarginReason,
    onLowMarginReasonChange,
    lowMarginEvidence,
    onLowMarginEvidenceChange,
    lowMarginPending,
    onLowMarginConfirm,
    changeConfirmOpen,
    onChangeConfirmOpenChange,
    onChangeConfirm,
}: {
    order: SalesOrderDetailView
    isCard: boolean
    voidOpen: boolean
    onVoidOpenChange: (open: boolean) => void
    voidPending: boolean
    onVoidConfirm: (reason: string) => Promise<void>
    lowMarginOpen: boolean
    onLowMarginOpenChange: (open: boolean) => void
    lowMarginReason: string
    onLowMarginReasonChange: (value: string) => void
    lowMarginEvidence: string
    onLowMarginEvidenceChange: (value: string) => void
    lowMarginPending: boolean
    onLowMarginConfirm: () => Promise<void>
    changeConfirmOpen: boolean
    onChangeConfirmOpenChange: (open: boolean) => void
    onChangeConfirm: () => Promise<void>
}) {
    return (
        <>
            <VoidSalesOrderDialog
                open={voidOpen}
                onOpenChange={onVoidOpenChange}
                pending={voidPending}
                onConfirm={onVoidConfirm}
            />

            <FormalActionConfirmDialog
                open={lowMarginOpen}
                onOpenChange={onLowMarginOpenChange}
                title="申请低毛利承接"
                actionLabel="提交承接申请"
                fromStatus={{ label: "采购未通过", tone: "warning" }}
                toStatus={{ label: "待销售上级确认", tone: "info" }}
                description={
                    <div className="space-y-2 text-left">
                        <Textarea
                            value={lowMarginReason}
                            onChange={(event) =>
                                onLowMarginReasonChange(event.target.value)
                            }
                            placeholder="说明维持原商业条件并由公司承接低毛利的理由"
                        />
                        <Input
                            value={lowMarginEvidence}
                            onChange={(event) =>
                                onLowMarginEvidenceChange(event.target.value)
                            }
                            placeholder="已登记证据 ID；多个以逗号分隔"
                        />
                    </div>
                }
                lockedFields={["原商业条件", "被驳回提交"]}
                effects={["冻结新提交", "创建销售上级低毛利确认待办"]}
                nextDepartment="销售上级"
                pending={lowMarginPending}
                onConfirm={onLowMarginConfirm}
            />

            <FormalActionConfirmDialog
                open={changeConfirmOpen}
                onOpenChange={onChangeConfirmOpenChange}
                title="发起改单"
                actionLabel="创建改单"
                confirmLabel="确认创建"
                fromStatus={{
                    label: `当前 v${order.version}`,
                    tone: "success",
                }}
                toStatus={{ label: "改单草稿", tone: "warning" }}
                lockedFields={["销售单号", "订单类型", "已生效版本"]}
                effects={[
                    "生成一笔改单，不改掉当前客户正在执行的版本",
                    "已有交付、回款、开票记录都会保留",
                    isCard
                        ? "卡券：运营确认影响 → 财务复核后新版本生效"
                        : "实物/服务：采购确认影响 → 财务复核后新版本生效",
                ]}
                nextDepartment={isCard ? "运营与财务" : "采购与财务"}
                onConfirm={onChangeConfirm}
            />
        </>
    )
}
