import assert from "node:assert/strict"
import test from "node:test"

import {
    canOpenWorkItemHandler,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./navigation-eligibility.ts"

test("W01 treats START_PROCESSING only as handler navigation eligibility", () => {
    assert.equal(canOpenWorkItemHandler(["START_PROCESSING"], false), true)
    assert.equal(canOpenWorkItemHandler(["PROCESS"], false), true)
    assert.equal(canOpenWorkItemHandler(["VIEW"], false), false)
    assert.equal(canOpenWorkItemHandler(["START_PROCESSING"], true), false)
})
