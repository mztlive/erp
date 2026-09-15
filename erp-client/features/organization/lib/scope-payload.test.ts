import { expect, it } from "vitest"

import {
    isRegisteredIdentifier,
    isStableIdentity,
    registeredResources,
    validateCreateDataScope,
} from "./scope-payload"

it("拒绝目标通配和显示名身份", () => {
    expect(isRegisteredIdentifier("sales_order")).toBe(true)
    expect(isRegisteredIdentifier("*")).toBe(false)
    expect(isRegisteredIdentifier("销售单")).toBe(false)
    expect(isStableIdentity("role-sales")).toBe(true)
    expect(isStableIdentity("销售经理")).toBe(false)
    expect(
        validateCreateDataScope({
            subjectType: "role",
            subjectId: "销售经理",
            scopeType: "company",
            resource: "sales_order",
            actions: ["list"],
            targetDimension: "internal_org",
            targetMode: null,
            includeDescendants: null,
            scopeTargets: [],
        }),
    ).toMatch(/稳定 ID/)
    expect(
        validateCreateDataScope({
            subjectType: "role",
            subjectId: "role-sales",
            scopeType: "organization",
            resource: "sales_order",
            actions: ["list"],
            targetDimension: "internal_org",
            targetMode: "explicit",
            includeDescendants: false,
            scopeTargets: ["*"],
        }),
    ).toMatch(/通配符/)
})

it("注册资源动作不含通配", () => {
    for (const item of registeredResources()) {
        expect(isRegisteredIdentifier(item.resource)).toBe(true)
        expect(item.actions.every(isRegisteredIdentifier)).toBe(true)
        expect(item.resource.includes("*")).toBe(false)
    }
})
