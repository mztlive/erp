import { describe, expect, it } from "vitest"

import type { AllocationLine } from "@/features/customer-receivables/types"

import {
    amountAllocatedToTargets,
    mapOrderInvoices,
    mapOrderReceipts,
    mapOrderReceivableAccounts,
    receivableTargetIds,
    remainingReceivableAmount,
} from "./sales-order-receivable"

function allocation(
    input: Pick<AllocationLine, "targetId" | "amountGross" | "action">,
): AllocationLine {
    return {
        allocationId: `al-${input.targetId}-${input.amountGross}`,
        action: input.action,
        amountGross: input.amountGross,
        targetLabel: input.targetId,
        targetId: input.targetId,
        occurredAt: "2026-04-01T00:00:00.000Z",
        isPosted: true,
    }
}

describe("remainingReceivableAmount", () => {
    it("subtracts received from gross", () => {
        expect(remainingReceivableAmount("1000.00", "250.50")).toBe("749.50")
    })

    it("falls back to gross when the input is not a decimal", () => {
        expect(remainingReceivableAmount("not-a-number", "1.00")).toBe(
            "not-a-number",
        )
    })
})

describe("amountAllocatedToTargets", () => {
    it("nets posted apply and reverse lines that hit this order", () => {
        expect(
            amountAllocatedToTargets(
                [
                    allocation({
                        targetId: "acc-1",
                        amountGross: "300.00",
                        action: "APPLY",
                    }),
                    allocation({
                        targetId: "entry-1",
                        amountGross: "50.00",
                        action: "APPLY",
                    }),
                    allocation({
                        targetId: "acc-1",
                        amountGross: "20.00",
                        action: "REVERSE",
                    }),
                    allocation({
                        targetId: "other-order",
                        amountGross: "999.00",
                        action: "APPLY",
                    }),
                ],
                new Set(["acc-1", "entry-1"]),
            ),
        ).toBe("330.00")
    })

    it("returns zero when nothing landed on this order", () => {
        expect(
            amountAllocatedToTargets(
                [
                    allocation({
                        targetId: "other-order",
                        amountGross: "80.00",
                        action: "APPLY",
                    }),
                ],
                new Set(["acc-1"]),
            ),
        ).toBe("0.00")
    })
})

describe("order receivable document mapping", () => {
    it("projects the receivable account and uses open total", () => {
        const account = {
            accountId: "acc-1",
            accountSeq: 1,
            salesOrderNo: "SO-1",
            statusLabel: "部分核销",
            statusTone: "warning" as const,
            openTotal: "700.00",
            counterpartyPartyName: "结算公司",
            entries: [{ entryId: "entry-1" }],
        }

        expect(receivableTargetIds([account])).toEqual(
            new Set(["acc-1", "entry-1"]),
        )
        expect(mapOrderReceivableAccounts([account])).toEqual([
            {
                id: "acc-1",
                documentType: "应收子账",
                documentNumber: "SO-1 · 子账 #1",
                statusLabel: "部分核销",
                statusTone: "warning",
                amount: "700.00",
                amountLabel: "开放应收（含税）",
                owner: "结算公司",
            },
        ])
    })

    it("projects receipts and invoices with this-order allocation amounts", () => {
        const targetIds = new Set(["acc-1"])
        const receipt = {
            receiptId: "rcpt-1",
            receiptNo: "SK-1",
            statusLabel: "已过账",
            statusTone: "success" as const,
            counterpartyPartyName: "结算公司",
            allocations: [
                allocation({
                    targetId: "acc-1",
                    amountGross: "120.00",
                    action: "APPLY",
                }),
            ],
        }
        const invoice = {
            invoiceId: "inv-1",
            invoiceNo: "FP-1",
            invoiceKindLabel: "蓝字",
            statusLabel: "已登记",
            statusTone: "info" as const,
            counterpartyPartyName: "结算公司",
            allocations: [
                allocation({
                    targetId: "acc-1",
                    amountGross: "80.00",
                    action: "APPLY",
                }),
            ],
        }

        expect(mapOrderReceipts([receipt], targetIds)[0]).toMatchObject({
            id: "rcpt-1",
            documentType: "客户回款",
            amount: "120.00",
            amountLabel: "核到本单（含税）",
        })
        expect(mapOrderInvoices([invoice], targetIds)[0]).toMatchObject({
            id: "inv-1",
            documentType: "销项发票 · 蓝字",
            amount: "80.00",
        })
    })
})
