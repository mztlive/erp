"use client"

import { ReturnSettlementDialog } from "./return-settlement-dialog"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { MoneyValue, QuantityValue } from "@/components/business/values"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { compareDecimal } from "@/lib/fixed-decimal"
import type { PurchaseOrderCenterView } from "../types"
import { sourceReturnProgress } from "../lib/source-return-progress"
import { PurchaseDirectReturnDialog } from "./purchase-direct-return-dialog"

/** 只展示当前采购动作或阻碍；后续步骤不提前占用作业面。 */
export function PurchaseSourceReturnPanel({
    order,
    disabled = false,
    showAction = true,
}: {
    order: PurchaseOrderCenterView
    disabled?: boolean
    showAction?: boolean
}) {
    const progress = sourceReturnProgress(order)
    const { plan, physicalPending, refundPending, invoicePending } = progress
    if (!plan || !progress.blocksChange) return null
    const lines = plan.lines.filter(
        (line) => compareDecimal(line.required_return, "0", 6) > 0,
    )
    const canRegister = lines.some(
        (line) => compareDecimal(line.max_direct_return, "0", 6) > 0,
    )

    return (
        <section
            className="space-y-3 rounded-lg border p-4"
            aria-label="当前处理事项"
        >
            <h3 className="text-sm font-medium">
                {physicalPending
                    ? "确认退货结果"
                    : !progress.decisionCurrent
                      ? "确认退货款处理方式"
                      : "财务处理中"}
            </h3>
            {physicalPending ? (
                <>
                    <div className="divide-y">
                        {lines.map((line) => (
                            <div
                                key={line.purchase_line_id}
                                className="flex flex-wrap items-center justify-between gap-2 py-2 text-sm"
                            >
                                <div>
                                    <p className="font-medium">
                                        {line.item_name}
                                    </p>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        采购{" "}
                                        <QuantityValue
                                            unit=""
                                            value={line.purchased_quantity}
                                        />{" "}
                                        → 本单目标{" "}
                                        <QuantityValue
                                            unit=""
                                            value={line.target_quantity}
                                        />{" "}
                                        {line.unit}
                                        {compareDecimal(
                                            line.returned_quantity,
                                            "0",
                                            6,
                                        ) > 0 ? (
                                            <>
                                                {" "}
                                                · 已登记退回{" "}
                                                <QuantityValue
                                                    unit=""
                                                    value={
                                                        line.returned_quantity
                                                    }
                                                />{" "}
                                                {line.unit}
                                            </>
                                        ) : null}
                                    </p>
                                </div>
                                <span>
                                    待确认退回{" "}
                                    <QuantityValue
                                        unit=""
                                        value={line.required_return}
                                    />{" "}
                                    {line.unit}
                                </span>
                            </div>
                        ))}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        {canRegister
                            ? "请与供应商核实收货结果；已收到的数量在此登记。"
                            : "请核对原发货来源；当前没有可登记的供应商直发数量。"}
                    </p>
                    {showAction ? (
                        <PurchaseSourceReturnAction
                            order={order}
                            disabled={disabled}
                        />
                    ) : null}
                </>
            ) : (
                <>
                    {!order.currentContent.costMasked ? (
                        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
                            {refundPending ? (
                                <>
                                    <dt className="text-muted-foreground">
                                        待处理款项差额
                                    </dt>
                                    <dd>
                                        <MoneyValue value={progress.refund} />
                                    </dd>
                                </>
                            ) : null}
                            {invoicePending ? (
                                <>
                                    <dt className="text-muted-foreground">
                                        预计需冲减进项票额
                                    </dt>
                                    <dd>
                                        <MoneyValue
                                            value={progress.redInvoice}
                                        />
                                    </dd>
                                </>
                            ) : null}
                        </dl>
                    ) : null}
                    {progress.decisionCurrent ? (
                        <div className="space-y-2 text-sm">
                            {(plan.finance_tasks ?? []).map((task) => (
                                <p key={task.work_item_id}>
                                    {task.kind === "PURCHASE_RETURN_REFUND"
                                        ? "退货款处理"
                                        : "进项红票"}{" "}
                                    · {task.owner_name} 正在办理
                                </p>
                            ))}
                            {(plan.settlements ?? []).map((record) => (
                                <p
                                    key={record.id}
                                    className="text-muted-foreground"
                                >
                                    {record.method === "SUPPLIER_CREDIT"
                                        ? "留抵"
                                        : "退款"}{" "}
                                    <MoneyValue value={record.amount} /> ·{" "}
                                    {record.status === "posted"
                                        ? "已入账"
                                        : "审批中"}
                                </p>
                            ))}
                        </div>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            请先与供应商确定退款或留抵及金额，再交给财务。
                        </p>
                    )}
                    {showAction ? (
                        <PurchaseSourceReturnAction
                            order={order}
                            disabled={disabled}
                        />
                    ) : null}
                </>
            )}
        </section>
    )
}

/** 工作台动作放入固定底栏；详情页复用同一登记入口。 */
export function PurchaseSourceReturnAction({
    order,
    disabled = false,
}: {
    order: PurchaseOrderCenterView
    disabled?: boolean
}) {
    const [open, setOpen] = useState(false)
    const profile = useAccountProfileQuery()
    const { plan, physicalPending, refund, blocksChange } =
        sourceReturnProgress(order)
    if (!plan || !blocksChange) return null
    const awaitingApproval =
        plan.settlements?.some((record) => record.status === "in_approval") ??
        false
    if (!physicalPending && awaitingApproval)
        return (
            <span className="text-xs text-muted-foreground">
                款项审批中；如需变更方案，请先联系财务经办人撤回。
            </span>
        )
    const canRegister = plan.lines.some(
        (line) => compareDecimal(line.max_direct_return, "0", 6) > 0,
    )
    if (physicalPending && !canRegister) return null
    const canReturn = hasPermission(
        profile.data?.permissions,
        "purchase_return_order:create",
    )
    if (profile.isPending) return null
    if (!canReturn)
        return (
            <span className="text-xs text-muted-foreground">
                由具备采购退货权限的经办人登记。
            </span>
        )
    return (
        <>
            <Button
                id="purchase-source-register-return"
                disabled={disabled}
                onClick={() => setOpen(true)}
            >
                {physicalPending
                    ? "登记供应商已收货"
                    : plan.settlement_decision
                      ? "调整剩余款项方案"
                      : "确认款项处理方式"}
            </Button>
            {open && !physicalPending ? (
                <ReturnSettlementDialog
                    purchaseOrderId={order.identity.purchaseOrderId}
                    plan={plan}
                    excess={refund}
                    onClose={() => setOpen(false)}
                />
            ) : open ? (
                <PurchaseDirectReturnDialog
                    purchaseOrderId={order.identity.purchaseOrderId}
                    version={order.identity.lockVersion}
                    plan={plan}
                    onClose={() => setOpen(false)}
                />
            ) : null}
        </>
    )
}
