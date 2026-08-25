import assert from "node:assert/strict"
import test from "node:test"

import {
    splitDetailSections,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./detail-facts.ts"

test("source sales order stays a key field and keeps its routing id", () => {
    const facts = splitDetailSections(
        [
            { label: "供应商", value: "华东纸业" },
            {
                label: "来源销售单",
                value: "SO-1",
                objectId: "so-1",
            },
            { label: "含税金额", value: "¥12,800", numeric: true },
            { label: "付款条件", value: "先款 30%" },
            { label: "采购类型", value: "实物" },
        ],
        "华东纸业",
    )
    assert.deepEqual(facts.keyFields[0], {
        label: "来源销售单",
        value: "SO-1",
        objectId: "so-1",
    })
    assert.equal(
        facts.keyFields.some((section) => section.label === "付款条件"),
        true,
    )
    assert.equal(
        facts.moreFields.some((section) => section.label === "来源销售单"),
        false,
    )
    assert.equal(facts.amounts[0]?.label, "含税金额")
})
