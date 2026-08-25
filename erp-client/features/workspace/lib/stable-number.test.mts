import assert from "node:assert/strict"
import test from "node:test"

import {
    stripDocumentNumberPrefix,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./stable-number.ts"

test("strips a Chinese document type before a coded number", () => {
    assert.equal(
        stripDocumentNumberPrefix("销售单 XS20260825170146"),
        "XS20260825170146",
    )
    assert.equal(stripDocumentNumberPrefix("回款 RC-0203"), "RC-0203")
    assert.equal(stripDocumentNumberPrefix("采购单 CG2026 001"), "CG2026 001")
})

test("keeps numbers that are already bare", () => {
    assert.equal(
        stripDocumentNumberPrefix("XS20260825175244"),
        "XS20260825175244",
    )
    assert.equal(stripDocumentNumberPrefix("  XS-1  "), "XS-1")
})
