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
            className="space-y-3 rounded-lg border p-4 @container/return-lines"
            aria-label="当前处理事项"
        >
            <h3 className="text-base font-semibold">
                {physicalPending
                    ? "当前需要你处理：核实退货收货结果"
                    : !progress.decisionCurrent
                      ? "确认退货款处理方式"
                      : "财务处理中"}
            </h3>
            {physicalPending ? (
                <>
                    <p className="text-sm leading-relaxed">
                        {canRegister
                            ? "请与销售及供应商核实客户退回商品的接收结果，再登记供应商实际收到的数量和收货凭据。"
                            : "请核对商品原发货来源，并按对应退货流程办理。当前没有可在此登记的供应商直发数量。"}
                    </p>
                    <div className="divide-y">
                        {lines.map((line) => (
                            <article
                                key={line.purchase_line_id}
                                className="space-y-3 py-4"
                            >
                                <h4 className="text-sm font-medium">
                                    {line.item_name}
                                </h4>
                                <dl className="grid grid-cols-1 gap-3 @sm/return-lines:grid-cols-3">
                                    <div className="space-y-1 rounded-lg bg-muted/50 p-3">
                                        <dt className="text-sm text-muted-foreground">
                                            本采购当前数量
                                        </dt>
                                        <dd className="text-lg">
                                            <QuantityValue
                                                value={line.purchased_quantity}
                                                unit={line.unit}
                                            />
                                        </dd>
                                    </div>
                                    <div className="space-y-1 rounded-lg bg-muted/50 p-3">
                                        <dt className="text-sm text-muted-foreground">
                                            调整后目标数量
                                        </dt>
                                        <dd className="text-lg">
                                            <QuantityValue
                                                value={line.target_quantity}
                                                unit={line.unit}
                                            />
                                        </dd>
                                    </div>
                                    <div className="space-y-1 rounded-lg border border-warning-border bg-warning-soft p-3 text-warning-soft-foreground">
                                        <dt className="text-sm">
                                            尚待确认退回
                                        </dt>
                                        <dd className="text-lg font-semibold">
                                            <QuantityValue
                                                value={line.required_return}
                                                unit={line.unit}
                                            />
                                        </dd>
                                    </div>
                                </dl>
                                <p className="text-sm text-muted-foreground">
                                    已登记供应商收到退货：
                                    <QuantityValue
                                        value={line.returned_quantity}
                                        unit={line.unit}
                                    />
                                </p>
                            </article>
                        ))}
                    </div>
                    {canRegister ? (
                        <div className="space-y-2 border-t pt-3 text-sm leading-relaxed">
                            <p>
                                供应商尚未收到退货时，暂不登记，本待办继续保留。支持分批登记，按本次实际收到的数量填写。
                            </p>
                            <p className="text-muted-foreground">
                                本次登记仅记录退货收货结果，不会直接修改采购数量或结束待办。登记后请按更新后的当前事项继续处理。
                            </p>
                        </div>
                    ) : null}
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
                    ? "登记供应商收到的退货"
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
