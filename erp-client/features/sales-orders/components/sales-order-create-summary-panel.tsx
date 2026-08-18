"use client"

import {
    MoneyValue,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { calculateTotals } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"

export type SalesOrderCreateSummaryPanelProps = {
    form: SalesOrderCreateFormApi
}

export function SalesOrderCreateSummaryPanel({
    form,
}: SalesOrderCreateSummaryPanelProps) {
    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => {
                const totals = calculateTotals(
                    values.lineItems,
                    values.taxRatePercent,
                )
                const natureLabel =
                    values.nature === "card_voucher" ? "卡券" : "实物/服务"
                const nextStep =
                    values.nature === "card_voucher"
                        ? "提交后进入销售领导 → 运营两级审批"
                        : "提交后进入审批"
                return (
                    <div
                        className={cn(
                            surfacePanelClassName,
                            "sticky top-14 space-y-4 p-4",
                        )}
                    >
                        <div>
                            <h2 className="font-heading text-sm font-semibold">
                                本单摘要
                            </h2>
                            <p className="mt-1 text-xs text-muted-foreground">
                                随填写实时更新
                            </p>
                        </div>
                        <dl className="space-y-2.5 text-xs">
                            <div className="flex justify-between gap-2">
                                <dt className="text-muted-foreground">合同</dt>
                                <dd className="max-w-[10rem] truncate text-right font-medium">
                                    {values.contractRevisionLabel || "未选择"}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-2">
                                <dt className="text-muted-foreground">客户</dt>
                                <dd className="max-w-[10rem] truncate text-right font-medium">
                                    {values.customerName || "—"}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-2">
                                <dt className="text-muted-foreground">结算</dt>
                                <dd className="max-w-[10rem] truncate text-right font-medium">
                                    {values.settlementEntity || "—"}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-2">
                                <dt className="text-muted-foreground">
                                    业务性质
                                </dt>
                                <dd className="font-medium">{natureLabel}</dd>
                            </div>
                            <div className="flex justify-between gap-2">
                                <dt className="text-muted-foreground">
                                    明细行
                                </dt>
                                <dd className="font-medium">
                                    {values.lineItems.length} 行
                                </dd>
                            </div>
                            <div className="border-t border-border/30 pt-3">
                                <div className="flex justify-between gap-2">
                                    <dt className="text-muted-foreground">
                                        含税预估
                                    </dt>
                                    <dd className="num font-semibold">
                                        <MoneyValue
                                            value={totals.gross}
                                            taxBasis="gross"
                                        />
                                    </dd>
                                </div>
                                <div className="mt-2 flex justify-between gap-2">
                                    <dt className="text-muted-foreground">
                                        税额
                                    </dt>
                                    <dd className="num">
                                        <MoneyValue value={totals.tax} />
                                    </dd>
                                </div>
                            </div>
                        </dl>
                        <p
                            className={cn(
                                surfaceInsetClassName,
                                "px-2.5 py-2 text-xs leading-relaxed text-muted-foreground",
                            )}
                        >
                            {nextStep}
                        </p>
                    </div>
                )
            }}
        </form.Subscribe>
    )
}
