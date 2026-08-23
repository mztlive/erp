import { describe, expect, it } from "vitest"

import {
    buildDecisionRequest,
    DECISION_REQUEST_KEYS,
    filterAllowedActions,
    filterRecoveryOptions,
    mapDocumentApprovalViewDto,
    mapHistoryItemDto,
    requestKeysOf,
    type DocumentApprovalViewDto,
} from "./types"

describe("buildDecisionRequest", () => {
    it("only emits the contract whitelist and keeps approve reason optional", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-1",
            decision: "APPROVE",
            expectedTaskVersion: "3",
            idempotencyKey: "k1",
        })
        expect(requestKeysOf(request)).toEqual(
            [...DECISION_REQUEST_KEYS].filter((key) => key !== "reason").sort(),
        )
        expect(request).toEqual({
            work_item_id: "wi-1",
            decision: "APPROVE",
            expected_task_version: "3",
            idempotency_key: "k1",
        })
    })

    it("includes a trimmed reject reason and never next-node fields", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-1",
            decision: "REJECT",
            reason: " 资料不全 ",
            expectedTaskVersion: "3",
            idempotencyKey: "k2",
        })
        expect(requestKeysOf(request)).toEqual(
            [...DECISION_REQUEST_KEYS].sort(),
        )
        expect(request).not.toHaveProperty("next_node")
        expect(request).not.toHaveProperty("reject_target")
        expect(request).not.toHaveProperty("next_assignee")
        expect(request.reason).toBe("资料不全")
    })
})

describe("filterAllowedActions / recovery options", () => {
    it("drops unknown actions instead of inventing defaults", () => {
        expect(
            filterAllowedActions([
                "APPROVE",
                "REASSIGN",
                "REASSIGN_CURRENT_APPROVER",
                "RETRY_CURRENT_STEP",
            ]),
        ).toEqual(["APPROVE"])
        expect(
            filterRecoveryOptions([
                "RESUME_CURRENT_APPROVER",
                "REASSIGN_CURRENT_APPROVER",
                "RETRY_CURRENT_STEP",
                "CANCEL_BLOCKED_APPROVAL",
            ]),
        ).toEqual(["RESUME_CURRENT_APPROVER", "CANCEL_BLOCKED"])
    })
})

describe("mapDocumentApprovalViewDto", () => {
    it("maps a created binding without turning it into a work item", () => {
        const dto: DocumentApprovalViewDto = {
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-1",
                name: "库存调整审批",
                version: 3,
                nodes: [
                    { key: "n1", name: "销售审核", assignee_name: "张三" },
                    { key: "n2", name: "财务审核", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        }
        const view = mapDocumentApprovalViewDto(dto)
        expect(view.instance).toBeUndefined()
        expect(view.recentHistory).toEqual([])
        expect(view.definition?.nodes).toHaveLength(2)
        expect(view.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("keeps cross-round history instead of collapsing by node key", () => {
        const first = mapHistoryItemDto({
            execution_id: "ex-1",
            round_no: 1,
            execution_no: 1,
            node_key: "n1",
            node_name: "销售审核",
            result: "REJECTED",
            decision_reason: "资料不全",
        })
        const second = mapHistoryItemDto({
            execution_id: "ex-2",
            round_no: 2,
            execution_no: 1,
            node_key: "n1",
            node_name: "销售审核",
            result: "ACTIVE",
        })
        expect(first.nodeKey).toBe(second.nodeKey)
        expect(first.executionId).not.toBe(second.executionId)
        expect(second.roundNo).toBe(first.roundNo + 1)
    })
})
