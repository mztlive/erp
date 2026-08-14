import { describe, it, expect } from "vitest"

import { containsAction, toResponsibilityStatus } from "./responsibility"
import { makeQueueItem } from "../hooks/test-fixtures"

describe("toResponsibilityStatus", () => {
    it("maps completed and closed items to terminal states first", () => {
        expect(
            toResponsibilityStatus(makeQueueItem({ status: "COMPLETED" }), "mine"),
        ).toBe("completed")
        expect(
            toResponsibilityStatus(makeQueueItem({ status: "CLOSED" }), "mine"),
        ).toBe("closed")
    })

    it("maps approval-blocked items", () => {
        expect(
            toResponsibilityStatus(
                makeQueueItem({ processingState: "APPROVAL_BLOCKED" }),
                "mine",
            ),
        ).toBe("blocked")
    })

    it("maps unowned pool items to pool_available", () => {
        expect(
            toResponsibilityStatus(
                makeQueueItem({ assignmentMode: "POOL", ownerUser: undefined }),
                "team",
            ),
        ).toBe("pool_available")
    })

    it("assigns to me or to others based on scope and the current user", () => {
        const item = makeQueueItem({ ownerUser: { id: "me", displayName: "我" } })

        expect(toResponsibilityStatus(item, "mine", "someone-else")).toBe(
            "assigned_to_me",
        )
        expect(toResponsibilityStatus(item, "team", "someone-else")).toBe(
            "assigned_to_other",
        )
        expect(toResponsibilityStatus(item, "team", "me")).toBe("assigned_to_me")
        expect(toResponsibilityStatus(item, "managed", "me")).toBe(
            "assigned_to_me",
        )
    })
})

describe("containsAction", () => {
    it("reflects the allowed action set", () => {
        const item = makeQueueItem({ allowedActions: ["CLOSE"] })

        expect(containsAction(item, "CLOSE")).toBe(true)
        expect(containsAction(item, "REASSIGN")).toBe(false)
        expect(containsAction(item, "START_PROCESSING")).toBe(false)
    })
})
