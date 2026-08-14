import assert from "node:assert/strict"
import test from "node:test"

import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "../lib/formal-command.ts"
import {
    fromFetchError,
    fromHttpResponse,
    fromParse,
} from "../lib/api/errors.ts"

test("unknown result reuses the original key and immutable payload", () => {
    let sequence = 0
    const ledger = new FormalCommandKeyLedger(
        (prefix) => `${prefix}:test-${++sequence}`,
    )
    const input = { decision: "APPROVE", evidence: ["file-1"] }
    const first = ledger.acquire("review", "purchase-review", input)

    input.decision = "REJECT"
    input.evidence.push("file-2")
    ledger.settle("review", "unknown")

    const retry = ledger.acquire("review", "purchase-review", input)
    assert.equal(retry.idempotencyKey, first.idempotencyKey)
    assert.deepEqual(retry.payload, {
        decision: "APPROVE",
        evidence: ["file-1"],
    })
})

test("success and definite failure start a new command identity", () => {
    let sequence = 0
    const ledger = new FormalCommandKeyLedger(
        (prefix) => `${prefix}:test-${++sequence}`,
    )
    const first = ledger.acquire("submit", "sales-submit", { version: 1 })
    ledger.settle("submit", "succeeded")
    const second = ledger.acquire("submit", "sales-submit", { version: 2 })
    ledger.settle("submit", "failed")
    const third = ledger.acquire("submit", "sales-submit", { version: 2 })

    assert.notEqual(second.idempotencyKey, first.idempotencyKey)
    assert.notEqual(third.idempotencyKey, second.idempotencyKey)
})

test("network, parse and 5xx failures are unknown while 4xx is definite", () => {
    assert.equal(
        classifyFormalCommandError(fromFetchError(new Error("offline"))),
        "unknown",
    )
    assert.equal(
        classifyFormalCommandError(fromParse(new SyntaxError("invalid json"))),
        "unknown",
    )
    assert.equal(classifyFormalCommandError(fromHttpResponse(503)), "unknown")
    assert.equal(
        classifyFormalCommandError(
            fromHttpResponse(409, {
                success: false,
                errorMessage: "任务版本已变化",
            }),
        ),
        "failed",
    )
    assert.equal(
        classifyFormalCommandError(
            fromHttpResponse(422, {
                success: false,
                errorMessage: "提交内容不符合要求",
            }),
        ),
        "failed",
    )
})
