import assert from "node:assert/strict"
import test from "node:test"

import {
    canOpenWorkItemHandler,
    isApprovalWorkbenchTask,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./navigation-eligibility.ts"

test("workbench no longer treats start-processing as a page action", () => {
    assert.equal(canOpenWorkItemHandler(["START_PROCESSING"], false), false)
    assert.equal(canOpenWorkItemHandler(["PROCESS"], false), true)
    assert.equal(canOpenWorkItemHandler(["OPEN_DOCUMENT"], false), true)
    assert.equal(canOpenWorkItemHandler(["VIEW"], false), true)
    assert.equal(canOpenWorkItemHandler(["APPROVE"], false), false)
})

test("approval tasks are identified from server actions or instance id", () => {
    assert.equal(
        isApprovalWorkbenchTask(["APPROVE"], undefined, undefined),
        true,
    )
    assert.equal(isApprovalWorkbenchTask(["VIEW"], "inst-1", undefined), true)
    assert.equal(isApprovalWorkbenchTask(["VIEW"], undefined, undefined), false)
})
