import { describe, expect, it } from "vitest"

import {
    buildSaveSelectionSchema,
    createBookSchema,
    quantityStringSchema,
    tierRuleSchema,
} from "@/features/sales-selection/lib/validation"

describe("sales selection validation", () => {
    it("accepts a single-sku by-quantity book from filter source", () => {
        const parsed = createBookSchema.safeParse({
            customer_id: "cust_1",
            sales_owner_user_id: "user_1",
            business_org_unit_id: "org_1",
            selection_form: "SINGLE_SKU",
            submit_mode: "BY_QUANTITY",
            source_kind: "FILTER",
            filter_q: "tea",
            tiers: [],
        })
        expect(parsed.success).toBe(true)
    })

    it("rejects books without explicit sales responsibility", () => {
        expect(
            createBookSchema.safeParse({
                customer_id: "cust_1",
                sales_owner_user_id: "",
                business_org_unit_id: "org_1",
                selection_form: "SINGLE_SKU",
                submit_mode: "BY_QUANTITY",
                source_kind: "FILTER",
            }).success,
        ).toBe(false)
        expect(
            createBookSchema.safeParse({
                customer_id: "cust_1",
                sales_owner_user_id: "user_1",
                business_org_unit_id: "",
                selection_form: "SINGLE_SKU",
                submit_mode: "BY_QUANTITY",
                source_kind: "FILTER",
            }).success,
        ).toBe(false)
    })

    it("rejects books without customer, form or mode", () => {
        const parsed = createBookSchema.safeParse({
            customer_id: "",
            selection_form: "SINGLE_SKU",
            submit_mode: "BY_QUANTITY",
            source_kind: "FILTER",
        })
        expect(parsed.success).toBe(false)
    })

    it("requires tiers for package form with unique names", () => {
        const tier = {
            name: "100 元档",
            target_amount: "100.00",
            tolerance: "5.00",
            expected_count: 5,
            sku_count: 3,
        }
        expect(
            createBookSchema.safeParse({
                customer_id: "cust_1",
                sales_owner_user_id: "user_1",
                business_org_unit_id: "org_1",
                selection_form: "PACKAGE",
                submit_mode: "BY_QUANTITY",
                source_kind: "FILTER",
                tiers: [],
            }).success,
        ).toBe(false)
        expect(
            createBookSchema.safeParse({
                customer_id: "cust_1",
                sales_owner_user_id: "user_1",
                business_org_unit_id: "org_1",
                selection_form: "PACKAGE",
                submit_mode: "BY_QUANTITY",
                source_kind: "FILTER",
                tiers: [tier, tier],
            }).success,
        ).toBe(false)
        expect(
            createBookSchema.safeParse({
                customer_id: "cust_1",
                sales_owner_user_id: "user_1",
                business_org_unit_id: "org_1",
                selection_form: "PACKAGE",
                submit_mode: "MALL_REDEEM",
                source_kind: "SELECTION",
                sku_ids: ["sku_a"],
                tiers: [tier],
            }).success,
        ).toBe(true)
    })

    it("rejects non-positive targets and negative tolerances", () => {
        expect(
            tierRuleSchema.safeParse({
                name: "A",
                target_amount: "0.00",
                tolerance: "0",
                expected_count: 5,
                sku_count: 3,
            }).success,
        ).toBe(false)
        expect(
            tierRuleSchema.safeParse({
                name: "A",
                target_amount: "100.00",
                tolerance: "-1.00",
                expected_count: 5,
                sku_count: 3,
            }).success,
        ).toBe(false)
    })

    it("rejects non-integer or out-of-range tier counts", () => {
        expect(
            tierRuleSchema.safeParse({
                name: "A",
                target_amount: "100.00",
                tolerance: "0",
                expected_count: 21,
                sku_count: 3,
            }).success,
        ).toBe(false)
        expect(
            tierRuleSchema.safeParse({
                name: "A",
                target_amount: "100.00",
                tolerance: "0",
                expected_count: 5,
                sku_count: 9,
            }).success,
        ).toBe(false)
    })

    it("validates quantities as integers within 1 to 100000", () => {
        expect(quantityStringSchema.safeParse("1").success).toBe(true)
        expect(quantityStringSchema.safeParse("100000").success).toBe(true)
        expect(quantityStringSchema.safeParse("0").success).toBe(false)
        expect(quantityStringSchema.safeParse("100001").success).toBe(false)
        expect(quantityStringSchema.safeParse("1.5").success).toBe(false)
    })

    it("rejects empty selections", () => {
        expect(
            buildSaveSelectionSchema("BY_QUANTITY").safeParse({
                selections: [],
            }).success,
        ).toBe(false)
    })

    it("requires quantities for by-quantity and forbids them for redeem", () => {
        expect(
            buildSaveSelectionSchema("BY_QUANTITY").safeParse({
                selections: [{ item_id: "item_1" }],
            }).success,
        ).toBe(false)
        expect(
            buildSaveSelectionSchema("BY_QUANTITY").safeParse({
                selections: [{ item_id: "item_1", quantity: "2" }],
            }).success,
        ).toBe(true)
        expect(
            buildSaveSelectionSchema("MALL_REDEEM").safeParse({
                selections: [{ item_id: "item_1", quantity: "2" }],
            }).success,
        ).toBe(false)
        expect(
            buildSaveSelectionSchema("MALL_REDEEM").safeParse({
                selections: [{ item_id: "item_1" }],
            }).success,
        ).toBe(true)
    })

    it("rejects duplicated items", () => {
        expect(
            buildSaveSelectionSchema("MALL_REDEEM").safeParse({
                selections: [{ item_id: "a" }, { item_id: "a" }],
            }).success,
        ).toBe(false)
    })
})
