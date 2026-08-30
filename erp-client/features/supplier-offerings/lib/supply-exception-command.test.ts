import { describe, expect, it } from "vitest"

import { supplyExceptionCompletionIntent } from "./supply-exception-command"

const payload = {
    offeringId: "offering-1",
    workItemId: "wi-1",
    expectedTaskVersion: "3",
    expectedSubjectVersion: "revision-2",
    evidenceReference: "  supplier-letter-1  ",
    comment: "  已核对暂停影响  ",
}

describe("supplyExceptionCompletionIntent", () => {
    it("同一任务版本与规范化载荷复用同一账本槽位", () => {
        const first = supplyExceptionCompletionIntent(payload)
        const retry = supplyExceptionCompletionIntent({
            ...payload,
            evidenceReference: "supplier-letter-1",
            comment: "已核对暂停影响",
        })

        expect(retry).toEqual(first)
    })

    it("载荷或任务版本变化时形成新的正式意图", () => {
        const first = supplyExceptionCompletionIntent(payload)
        const changedPayload = supplyExceptionCompletionIntent({
            ...payload,
            comment: "已核对暂停影响并登记替代采购事项",
        })
        const changedVersion = supplyExceptionCompletionIntent({
            ...payload,
            expectedTaskVersion: "4",
        })

        expect(changedPayload.slot).not.toBe(first.slot)
        expect(changedVersion.slot).not.toBe(first.slot)
    })
})
