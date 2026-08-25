import { describe, expect, it } from "vitest"

import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

import {
    isSalesOrderApprovalInProgress,
    resolveFocusTask,
} from "./sales-order-detail-model"

function approval(instanceStatus?: string): DocumentApprovalView {
    return {
        requirement: "REQUIRED",
        recentHistory: [],
        historyHasMore: false,
        allowedActions: [],
        instance:
            instanceStatus == null
                ? undefined
                : {
                      id: "inst-1",
                      status: instanceStatus,
                      currentRoundNo: 1,
                  },
    }
}

function order(input: {
    nature?: SalesOrderListItem["nature"]
    statusCode: string
    instanceStatus?: string
    hasApproval?: boolean
}): SalesOrderListItem {
    return {
        nature: input.nature ?? "physical_service",
        primaryStatus: {
            code: input.statusCode,
            label: input.statusCode,
            tone: "info",
        },
        approval:
            input.hasApproval === false
                ? undefined
                : approval(input.instanceStatus),
    } as SalesOrderListItem
}

describe("isSalesOrderApprovalInProgress", () => {
    it("does not treat an approved instance as pending", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
            ),
        ).toBe(false)
    })

    it("treats a running instance as pending even after the document is effective", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "effective", instanceStatus: "RUNNING" }),
            ),
        ).toBe(true)
    })

    it("treats a blocked instance as pending", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "in_approval", instanceStatus: "BLOCKED" }),
            ),
        ).toBe(true)
    })

    it("keeps the review-stage fallback only when no instance exists yet", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "in_approval" }),
            ),
        ).toBe(true)
        expect(
            isSalesOrderApprovalInProgress(
                order({
                    statusCode: "in_approval",
                    instanceStatus: "APPROVED",
                }),
            ),
        ).toBe(false)
        expect(
            isSalesOrderApprovalInProgress(order({ statusCode: "effective" })),
        ).toBe(false)
    })

    it("ignores cancelled instances", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "draft", instanceStatus: "CANCELLED" }),
            ),
        ).toBe(false)
    })
})

describe("resolveFocusTask", () => {
    it("does not keep the approval banner after the instance has passed", () => {
        expect(
            resolveFocusTask(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
                false,
            ),
        ).toBeNull()
    })

    it("keeps the approval banner while the instance is still running", () => {
        expect(
            resolveFocusTask(
                order({
                    statusCode: "in_approval",
                    instanceStatus: "RUNNING",
                }),
                true,
            ),
        ).toMatchObject({
            id: "approval",
            title: "销售单等审批",
        })
    })

    it("falls through to acceptance once approval has finished", () => {
        expect(
            resolveFocusTask(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
                true,
            ),
        ).toMatchObject({
            id: "acceptance",
            title: "可以做客户验收",
        })
    })

    it("uses the voucher copy for a running card-voucher approval", () => {
        expect(
            resolveFocusTask(
                order({
                    nature: "card_voucher",
                    statusCode: "in_approval",
                    instanceStatus: "RUNNING",
                }),
                false,
            ),
        ).toMatchObject({
            id: "approval",
            title: "卡券销售等审批",
        })
    })
})
