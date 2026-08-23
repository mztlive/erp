import { beforeEach, describe, expect, it, vi } from "vitest"

import * as api from "./api"
import { buildDecisionRequest } from "./types"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

import { apiGet, apiPost } from "@/lib/api"

beforeEach(() => {
    vi.clearAllMocks()
})

describe("submitDecision", () => {
    it("posts only the decision whitelist to the decision endpoint", async () => {
        vi.mocked(apiPost).mockResolvedValue({
            instance_id: "inst-1",
            instance_status: "RUNNING",
            current_round_no: 2,
            current_node_name: "销售审核",
            current_assignee_name: "张三",
            latest_rejection_reason: "资料不全",
            outcome: "APPLIED",
        })
        const request = buildDecisionRequest({
            workItemId: "wi-1",
            decision: "REJECT",
            reason: "资料不全",
            expectedTaskVersion: "3",
            idempotencyKey: "k1",
        })
        const view = await api.submitDecision(request)
        expect(apiPost).toHaveBeenCalledWith("/admin/approval-decisions", {
            work_item_id: "wi-1",
            decision: "REJECT",
            reason: "资料不全",
            expected_task_version: "3",
            idempotency_key: "k1",
        })
        expect(view.currentRoundNo).toBe(2)
        expect(view.latestRejectionReason).toBe("资料不全")
    })
})

describe("recovery and withdraw ports", () => {
    it("resumes without a target user or retry action enum", async () => {
        vi.mocked(apiPost).mockResolvedValue({
            instance_id: "inst-1",
            instance_status: "RUNNING",
            current_round_no: 1,
            next_open_task: {
                work_item_id: "wi-new",
                task_version: 1,
                owner_user_id: "u1",
            },
            outcome: "APPLIED",
        })
        const view = await api.resumeCurrentApprover("inst-1", {
            expected_instance_version: "2",
            expected_execution_version: "4",
            expected_assignment_version: "1",
            idempotency_key: "k4",
        })
        expect(apiPost).toHaveBeenCalledWith(
            "/admin/approval-instances/inst-1/resume-current-approver",
            expect.not.objectContaining({
                target_user_id: expect.anything(),
                recovery_action: expect.anything(),
            }),
        )
        expect(view.nextOpenTask?.workItemId).toBe("wi-new")
        expect(view.nextOpenTask?.workItemId).not.toBe("wi-old")
    })

    it("withdraws through the business document resource", async () => {
        vi.mocked(apiPost).mockResolvedValue({
            instance_id: "inst-1",
            instance_status: "CANCELLED",
            current_round_no: 1,
            outcome: "APPLIED",
        })
        await api.cancelDocumentApproval({
            documentType: "StockAdjustment",
            documentId: "adj-1",
            request: {
                reason: "录错",
                expected_instance_version: "2",
                expected_execution_version: "3",
                expected_task_version: "4",
                idempotency_key: "k9",
            },
        })
        expect(apiPost).toHaveBeenCalledWith(
            "/admin/business-documents/StockAdjustment/adj-1/approval/cancel",
            expect.objectContaining({ reason: "录错" }),
        )
    })
})

describe("recovery options", () => {
    it("reads server recovery options without inventing retry", async () => {
        vi.mocked(apiGet).mockResolvedValue({
            instance_id: "inst-1",
            actions: ["RESUME_CURRENT_APPROVER", "REASSIGN_CURRENT_APPROVER"],
        })
        const options = await api.getRecoveryOptions("inst-1")
        expect(apiGet).toHaveBeenCalledWith(
            "/admin/approval-instances/inst-1/recovery-options",
        )
        expect(options.actions).toEqual(["RESUME_CURRENT_APPROVER"])
    })
})

describe("isApprovalConflict", () => {
    it("treats 409 as a responsibility or version change", () => {
        expect(api.isApprovalConflict({ status: 409 })).toBe(true)
        expect(api.approvalConflictMessage({ status: 409 })).toBe(
            "责任或版本已变化，请刷新后重新确认",
        )
        expect(api.isApprovalConflict({ status: 500 })).toBe(false)
    })
})
