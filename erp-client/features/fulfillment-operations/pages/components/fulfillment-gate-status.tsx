"use client"

import Link from "next/link"

import { BusinessStatusBadge, PrepaymentGate } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    paymentRegistrationHref,
    prepaymentGateCopy,
} from "@/features/fulfillment-operations/pages/lib/gate-copy"
import type { FulfillmentOperation } from "@/features/fulfillment-operations/types"
import { clampZeroFixed, subtractFixed } from "@/lib/fixed-decimal"

export type FulfillmentGateStatusProps = {
    operation: FulfillmentOperation
    currentUrl: string
    snapshotUpdatedAt: string
    showPaymentAction?: boolean
}

/**
 * 处理条上的先款条件徽章。
 * 无先款要求时退化为中性徽章；先款未到时不展开完整卡片，只在悬停时给详情。
 */
export function FulfillmentGateStatus({
    operation,
    currentUrl,
    snapshotUpdatedAt,
    showPaymentAction = true,
}: FulfillmentGateStatusProps) {
    const isShip = operation.operationType === "WAREHOUSE_SHIP"
    if (operation.gate.state === "NOT_APPLICABLE") {
        return (
            <BusinessStatusBadge
                context="list"
                id="prepayment-gate"
                tone="neutral"
                label={isShip ? "发货条件：无先款要求" : "无先款要求"}
                description={operation.gate.message}
            />
        )
    }
    const blocked = operation.gate.state === "BLOCKED"
    return (
        <PrepaymentGate
            id="prepayment-gate"
            presentation="badge"
            copy={prepaymentGateCopy(isShip)}
            condition={{
                kind: "amount",
                required: operation.gate.requiredAmount ?? "—",
                description: operation.gate.message,
            }}
            allocated={operation.gate.effectivePaidAmount ?? "—"}
            gap={
                blocked &&
                operation.gate.requiredAmount &&
                operation.gate.effectivePaidAmount
                    ? clampZeroFixed(
                          subtractFixed(
                              operation.gate.requiredAmount,
                              operation.gate.effectivePaidAmount,
                              { maxScale: 2, outputScale: 2 },
                          ),
                          { maxScale: 2, outputScale: 2 },
                      )
                    : "0"
            }
            updatedAt={{
                label: "刚刚",
                dateTime: snapshotUpdatedAt,
            }}
            allowed={operation.gate.state === "SATISFIED"}
            paymentAction={
                blocked && showPaymentAction ? (
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={paymentRegistrationHref(
                                    operation.source.purchaseOrderId,
                                    currentUrl,
                                )}
                            />
                        }
                    >
                        去登记付款
                    </Button>
                ) : undefined
            }
        />
    )
}
