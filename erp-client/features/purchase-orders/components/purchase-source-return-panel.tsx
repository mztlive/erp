"use client"
import { useState } from "react"
import Link from "next/link"
import { Button } from "@/components/ui/button"
import { MoneyValue, QuantityValue } from "@/components/business/values"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { compareDecimal } from "@/lib/fixed-decimal"
import type { PurchaseOrderCenterView } from "../types"
import { sourceReturnProgress } from "../lib/source-return-progress"
import { PurchaseDirectReturnDialog } from "./purchase-direct-return-dialog"

export function PurchaseSourceReturnPanel({
    order,
    disabled = false,
}: {
    order: PurchaseOrderCenterView
    disabled?: boolean
}) {
    const [open, setOpen] = useState(false)
    const profile = useAccountProfileQuery()
    const progress = sourceReturnProgress(order)
    const { plan, physicalPending, hasReturns, refundPending, invoicePending } =
        progress
    if (
        !plan ||
        (!physicalPending && !hasReturns && !refundPending && !invoicePending)
    )
        return null
    const canReturn = hasPermission(
        profile.data?.permissions,
        "purchase_return_order:create",
    )
    const canRegister = plan.lines.some(
        (line) => compareDecimal(line.max_direct_return, "0", 6) > 0,
    )
    const financeUrl = (view: string) =>
        `/finance/supplier-accounts?${new URLSearchParams({ view, supplierId: order.header.supplierId, purchaseOrderId: order.identity.purchaseOrderId, from: "workspace", returnTo: `/procurement/orders/${encodeURIComponent(order.identity.purchaseOrderId)}` })}`
    return (
        <section className="space-y-4 border-y py-5" aria-label="退货处理进度">
            <div>
                <h3 className="text-sm font-semibold">采购调整前的处理</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                    原采购与发货记录保留。按实际退回、款票处理、采购调整的顺序完成。
                </p>
            </div>
            <ol className="divide-y">
                <li className="space-y-3 py-4">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <h4 className="text-sm font-medium">
                            1. 客户退回，供应商确认接收
                        </h4>
                        <span className="text-xs text-muted-foreground">
                            {physicalPending
                                ? "待退回或登记"
                                : "退回数量已满足本次调整"}
                        </span>
                    </div>
                    <div className="space-y-3">
                        {plan.lines.map((line) => (
                            <div
                                key={line.purchase_line_id}
                                className="text-sm"
                            >
                                <p className="font-medium">{line.item_name}</p>
                                <p className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground">
                                    <span>
                                        采购{" "}
                                        <QuantityValue
                                            unit=""
                                            value={line.purchased_quantity}
                                        />{" "}
                                        {line.unit}
                                    </span>
                                    <span>
                                        调整目标{" "}
                                        <QuantityValue
                                            unit=""
                                            value={line.target_quantity}
                                        />{" "}
                                        {line.unit}
                                    </span>
                                    <span>
                                        已登记退回{" "}
                                        <QuantityValue
                                            unit=""
                                            value={line.returned_quantity}
                                        />{" "}
                                        {line.unit}
                                    </span>
                                    <span>
                                        还需退回{" "}
                                        <QuantityValue
                                            unit=""
                                            value={line.required_return}
                                        />{" "}
                                        {line.unit}
                                    </span>
                                </p>
                            </div>
                        ))}
                    </div>
                    {physicalPending ? (
                        <>
                            <p className="text-xs text-muted-foreground">
                                请先落实退货地址和接收结果。实际退回后在此登记，未退回时保留待办。当前入口处理客户直退供应商，不产生公司仓出入库。
                            </p>
                            {canRegister && canReturn ? (
                                <Button
                                    id="purchase-source-register-return"
                                    variant="outline"
                                    disabled={disabled}
                                    onClick={() => setOpen(true)}
                                >
                                    登记已退回供应商
                                </Button>
                            ) : (
                                <p className="text-xs text-muted-foreground">
                                    {!canReturn
                                        ? "请由具有采购退货权限的经办人登记。"
                                        : "当前未找到可直接关联本采购的直发数量；请核对发货来源和退货路线。"}
                                </p>
                            )}
                        </>
                    ) : null}
                    {hasReturns ? (
                        <p className="break-all text-xs text-muted-foreground">
                            退货记录：{plan.return_numbers.join("、")}
                        </p>
                    ) : null}
                </li>
                <li className="space-y-3 py-4">
                    <h4 className="text-sm font-medium">
                        2. 核对供应商退款与发票
                    </h4>
                    {!order.currentContent.costMasked ? (
                        <p className="text-sm">
                            按原单价和现有费用，调整后预计含税金额{" "}
                            <MoneyValue value={plan.target_gross} />
                            {refundPending ? (
                                <>
                                    ；已付款超过该金额{" "}
                                    <MoneyValue value={progress.refund} />
                                    ，请按实际到账提交部分退款审批。
                                </>
                            ) : (
                                "；当前未发现超付差额。"
                            )}
                        </p>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            由财务核对退款和发票差额。
                        </p>
                    )}
                    {invoicePending && !order.currentContent.costMasked ? (
                        <p className="text-sm">
                            已收票超过调整后金额{" "}
                            <MoneyValue value={progress.redInvoice} />
                            ，请先处理进项红票。
                        </p>
                    ) : null}
                    <div className="flex flex-wrap gap-2">
                        {refundPending ? (
                            <Button
                                variant="outline"
                                render={
                                    <Link
                                        id="purchase-source-return-refund"
                                        href={financeUrl("payment")}
                                    />
                                }
                            >
                                打开付款记录办理退款
                            </Button>
                        ) : null}
                        {invoicePending ? (
                            <Button
                                variant="outline"
                                render={
                                    <Link
                                        id="purchase-source-return-invoice"
                                        href={financeUrl("purchase_invoice")}
                                    />
                                }
                            >
                                打开供应商发票
                            </Button>
                        ) : null}
                    </div>
                    {refundPending || invoicePending ? (
                        <p className="text-xs text-muted-foreground">
                            由财务在关联记录上办理，审批通过后返回并刷新。预计差额按当前分配到本采购的款票计算。
                        </p>
                    ) : null}
                </li>
                <li className="space-y-2 pt-4">
                    <h4 className="text-sm font-medium">
                        3. 调整采购并完成待办
                    </h4>
                    <p className="text-sm text-muted-foreground">
                        {progress.blocksChange
                            ? "完成以上适用事项后，再按销售目标调整采购数量和收件分配。"
                            : "可以继续改单，按销售目标调整数量和收件分配，填写原因后提交审批。"}
                        改单生效时调整应付；退货登记不重复冲减金额。系统核对通过后结束本次依据更新待办。
                    </p>
                </li>
            </ol>
            {open ? (
                <PurchaseDirectReturnDialog
                    purchaseOrderId={order.identity.purchaseOrderId}
                    version={order.identity.lockVersion}
                    plan={plan}
                    onClose={() => setOpen(false)}
                />
            ) : null}
        </section>
    )
}
