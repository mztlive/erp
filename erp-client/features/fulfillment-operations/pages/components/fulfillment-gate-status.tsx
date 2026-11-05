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
    id?: string
    operation: FulfillmentOperation
    currentUrl: string
    snapshotUpdatedAt: string
    showPaymentAction?: boolean
    presentation?: "badge" | "panel"
}

/**
 * 处理条上的先款条件徽章。
 * 无先款要求时退化为中性徽章；先款未到时不展开完整卡片，只在悬停时给详情。
 */
export function FulfillmentGateStatus({
    id = "prepayment-gate",
    operation,
    currentUrl,
    snapshotUpdatedAt,
    showPaymentAction = true,
    presentation = "badge",
}: FulfillmentGateStatusProps) {
    if (operation.gate.state === "NOT_APPLICABLE") {
        return (
            <BusinessStatusBadge
                context="list"
                id={id}
                tone="neutral"
                label="无预付要求"
                description="付款与履约分别处理"
            />
        )
    }
    const blocked = operation.gate.state === "BLOCKED"
    return (
        <PrepaymentGate
            id={id}
            presentation={presentation}
            copy={prepaymentGateCopy()}
            condition={{
                kind: "amount",
                required: operation.gate.requiredAmount ?? "—",
                description: "付款不阻止履约确认",
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
                        id="fulfillment-operations-gate-register-payment"
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
                        申请采购付款
                    </Button>
                ) : undefined
            }
        />
    )
}
