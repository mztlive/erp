"use client"

import {
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { cn } from "@/lib/utils"
import {
    paymentTermLabel,
    welfareScenarioLabel,
} from "@/lib/business-options"
import { calculateTotals } from "@/features/sales-orders/lib/sales-order-create-model"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-validation"
import type { SalesOrderNature } from "@/features/sales-orders/types"

export type SalesOrderSubmitSnapshot = {
    customerName: string
    contractLabel: string
    settlementEntity: string
    nature: SalesOrderNature
    welfareScene: string
    paymentTerms: string
    fulfillmentMode: string
    taxRatePercent: string
    lineCount: number
    amountGross: string
    amountNet: string
    amountTax: string
}

export function buildSalesOrderSubmitSnapshot(
    values: CreateSalesOrderFormValues,
): SalesOrderSubmitSnapshot {
    const totals = calculateTotals(values.lineItems, values.taxRatePercent)
    return {
        customerName: values.customerName,
        contractLabel: values.contractRevisionLabel,
        settlementEntity: values.settlementEntity,
        nature: values.nature,
        welfareScene: values.welfareScene,
        paymentTerms: values.paymentTerms,
        fulfillmentMode: values.fulfillmentMode,
        taxRatePercent: values.taxRatePercent,
        lineCount: values.lineItems.filter(
            (line) => line.sku.trim() || line.name.trim(),
        ).length,
        amountGross: totals.gross,
        amountNet: totals.net,
        amountTax: totals.tax,
    }
}

/** 提交确认弹窗中的本单摘要；替代锁定字段 / 影响列表等套话。 */
export function SalesOrderSubmitConfirmSummary({
    snapshot,
}: {
    snapshot: SalesOrderSubmitSnapshot
}) {
    const natureLabel =
        snapshot.nature === "card_voucher" ? "卡券" : "实物与服务"
    const welfareLabel =
        welfareScenarioLabel(snapshot.welfareScene) || "—"
    const paymentLabel =
        paymentTermLabel(snapshot.paymentTerms) ||
        snapshot.paymentTerms ||
        "—"

    return (
        <section
            aria-label="本单摘要"
            className="space-y-3 rounded-xl border border-border bg-muted p-4"
        >
            <h3 className="text-sm font-medium text-foreground">本单摘要</h3>
            <DescriptionList columns="two" className="gap-x-4 gap-y-3">
                <DescriptionItem>
                    <DescriptionTerm>客户</DescriptionTerm>
                    <DescriptionDetails>
                        {snapshot.customerName || "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>合同</DescriptionTerm>
                    <DescriptionDetails>
                        {snapshot.contractLabel || "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>结算主体</DescriptionTerm>
                    <DescriptionDetails>
                        {snapshot.settlementEntity || "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>业务性质</DescriptionTerm>
                    <DescriptionDetails>{natureLabel}</DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>福利场景</DescriptionTerm>
                    <DescriptionDetails>{welfareLabel}</DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>付款条件</DescriptionTerm>
                    <DescriptionDetails>{paymentLabel}</DescriptionDetails>
                </DescriptionItem>
                {snapshot.nature === "physical_service" ? (
                    <DescriptionItem>
                        <DescriptionTerm>履约方式</DescriptionTerm>
                        <DescriptionDetails>
                            {snapshot.fulfillmentMode || "—"}
                        </DescriptionDetails>
                    </DescriptionItem>
                ) : null}
                <DescriptionItem>
                    <DescriptionTerm>税率</DescriptionTerm>
                    <DescriptionDetails>
                        {snapshot.taxRatePercent
                            ? `${snapshot.taxRatePercent}%`
                            : "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>明细行</DescriptionTerm>
                    <DescriptionDetails>
                        {snapshot.lineCount} 行
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>

            <dl
                className={cn(
                    surfaceInsetClassName,
                    "grid grid-cols-1 gap-2 px-3 py-2.5 sm:grid-cols-3",
                )}
            >
                <div className="min-w-0">
                    <dt className="text-xs text-muted-foreground">含税金额</dt>
                    <dd className="mt-0.5">
                        <MoneyValue
                            value={snapshot.amountGross}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div className="min-w-0">
                    <dt className="text-xs text-muted-foreground">不含税金额</dt>
                    <dd className="mt-0.5">
                        <MoneyValue
                            value={snapshot.amountNet}
                            taxBasis="net"
                        />
                    </dd>
                </div>
                <div className="min-w-0">
                    <dt className="text-xs text-muted-foreground">税额</dt>
                    <dd className="mt-0.5">
                        <MoneyValue value={snapshot.amountTax} />
                    </dd>
                </div>
            </dl>
        </section>
    )
}
