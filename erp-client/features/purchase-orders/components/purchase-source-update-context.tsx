"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { PurchaseOrderCenterView } from "../types"
import { sourceReturnProgress } from "../lib/source-return-progress"

/** 工作台先交代业务原因和付款限制，版本与完整影响按需查看。 */
export function PurchaseSourceUpdateContext({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const update = order.sourceUpdate
    const progress = sourceReturnProgress(order)
    if (!update) return null

    return (
        <section className="space-y-3" aria-label="销售变更对本采购的影响">
            <Alert variant={update.blocksPayment ? "warning" : "info"}>
                <AlertTitle>
                    {update.awaitingSubmission
                        ? "采购修改已保存，请核对后提交审批"
                        : progress.physicalPending
                          ? "销售数量减少，需要处理本采购多出的商品"
                          : "销售要求已变更，需要核对并调整本采购"}
                </AlertTitle>
                <AlertDescription>
                    {update.blocksPayment
                        ? "付款状态：本采购暂停付款申请和付款，待本次影响处理完成后解除。"
                        : "请按下方当前事项处理，系统会根据处理结果更新待办。"}
                </AlertDescription>
            </Alert>
            <details
                className="rounded-lg border px-4 py-3"
                open={!progress.blocksChange}
            >
                <summary
                    id="purchase-source-update-impact-details"
                    className="cursor-pointer text-sm font-medium focus-visible:outline-2 focus-visible:outline-offset-4"
                >
                    查看销售变更内容
                </summary>
                <div className="mt-3 space-y-3 text-sm">
                    <ul className="space-y-2">
                        {update.reasons.map((reason) => (
                            <li key={`${reason.lineNo}-${reason.description}`}>
                                <span className="font-medium">
                                    {reason.itemName}：
                                </span>
                                {reason.description}
                            </li>
                        ))}
                    </ul>
                    {update.currentSalesRevisionNo &&
                    update.purchaseSalesRevisionNo ? (
                        <p className="text-muted-foreground">
                            销售单当前为第 {update.currentSalesRevisionNo}{" "}
                            版，本采购引用第 {update.purchaseSalesRevisionNo}{" "}
                            版。
                        </p>
                    ) : null}
                </div>
            </details>
        </section>
    )
}
