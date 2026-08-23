import { describe, expect, it } from "vitest"

import {
    buildBatchView,
    instantToIso,
    mapIssueCode,
    mapObjectType,
    mapRowStatus,
    toBackendStatusFilter,
    toListItem,
} from "@/features/import-opening/api/mappers"
import type {
    BackendBatchDetail,
    BackendBatchListItem,
    BackendConfirmation,
    BackendRow,
} from "@/features/import-opening/api/mappers"

const baseBatch: BackendBatchDetail = {
    id: "b1",
    batch_no: "B-001",
    source_system_id: "sys-a",
    source_object_set: "CUSTOMER, SKU",
    baseline_date: "2025-01-01",
    import_rule_version: "v3",
    status: "pending_confirmation",
    total_rows: 10,
    success_rows: 5,
    failed_rows: 0,
    version: 7,
    created_at: 1735689600,
}

function makeWorkItem(
    overrides: Partial<NonNullable<BackendConfirmation["work_item"]>> = {},
): NonNullable<BackendConfirmation["work_item"]> {
    return {
        work_item_id: "w1",
        work_item_type: "IMPORT_BUSINESS_CONFIRMATION",
        task_version: "v1",
        subject_version: "s1",
        status: "OPEN",
        owner_role: "sales",
        owner_organization_id: "org1",
        processing_state: "READY",
        allowed_actions: ["PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"],
        action_blockers: [],
        handler_key: "import_business_confirmation",
        destination_workspace_id: "W18",
        ...overrides,
    }
}

function makeConfirmation(
    overrides: Partial<BackendConfirmation> = {},
): BackendConfirmation {
    return {
        id: "c1",
        batch_id: "b1",
        confirmation_scope: "sales",
        owner_role: "sales",
        batch_version: 7,
        trial_version: 3,
        status: "PENDING",
        work_item_id: "w1",
        work_item: makeWorkItem(),
        version: 1,
        created_at: 1735689600,
        ...overrides,
    }
}

describe("instantToIso", () => {
    it("converts seconds to ISO and handles empty input", () => {
        expect(instantToIso(0)).toBe("1970-01-01T00:00:00.000Z")
        expect(instantToIso(null)).toBe("")
        expect(instantToIso(undefined)).toBe("")
        expect(instantToIso(Number.NaN)).toBe("")
    })
})

describe("toBackendStatusFilter", () => {
    it("maps feature statuses to backend statuses", () => {
        expect(toBackendStatusFilter("RECEIVING")).toBe("pending_validation")
        expect(toBackendStatusFilter("SCANNING")).toBe("pending_validation")
        expect(toBackendStatusFilter("TRIAL_READY")).toBe("validating")
        expect(toBackendStatusFilter("CONFIRMATION_BLOCKED")).toBe(
            "pending_confirmation",
        )
        expect(toBackendStatusFilter("READY_TO_APPLY")).toBe("ready_to_apply")
        expect(toBackendStatusFilter("APPLYING")).toBe("importing")
        expect(toBackendStatusFilter("PARTIAL_SUCCESS")).toBe("partial_failed")
        expect(toBackendStatusFilter("CANCELLED")).toBe("failed")
    })

    it("passes through backend values and rejects unknown ones", () => {
        expect(toBackendStatusFilter("completed")).toBe("completed")
        expect(toBackendStatusFilter("all")).toBeUndefined()
        expect(toBackendStatusFilter(undefined)).toBeUndefined()
        expect(toBackendStatusFilter("nope")).toBeUndefined()
    })
})

describe("mapIssueCode", () => {
    it("recognizes known codes, including embedded ones", () => {
        expect(mapIssueCode("CUSTOMER_NOT_FOUND")).toBe("CUSTOMER_NOT_FOUND")
        expect(mapIssueCode("E_STOCK_QTY_INVALID")).toBe("STOCK_QTY_INVALID")
    })

    it("falls back to MAPPING_CONFLICT", () => {
        expect(mapIssueCode(null)).toBe("MAPPING_CONFLICT")
        expect(mapIssueCode(undefined)).toBe("MAPPING_CONFLICT")
        expect(mapIssueCode("TOTALLY_UNKNOWN")).toBe("MAPPING_CONFLICT")
    })
})

describe("mapObjectType", () => {
    it("recognizes codes and Chinese names", () => {
        expect(mapObjectType("CUSTOMER")).toBe("CUSTOMER")
        expect(mapObjectType("客户")).toBe("CUSTOMER")
        expect(mapObjectType("sku")).toBe("SKU")
        expect(mapObjectType("库存")).toBe("OPENING_STOCK")
        expect(mapObjectType("应收")).toBe("CARD_OPENING_AR")
        expect(mapObjectType("sales_order")).toBe("CARD_SALES_ORDER")
        expect(mapObjectType("unknown")).toBe("CUSTOMER")
    })
})

describe("mapRowStatus", () => {
    const base: BackendRow = {
        id: "r1",
        batch_id: "b1",
        source_object_type: "CUSTOMER",
        source_row_key: "1",
        parse_status: "valid",
        mapping_status: "mapped",
        import_status: "imported",
        version: 1,
        created_at: 1,
    }

    it("maps conflict, pending mapping, failed, skipped and invalid rows", () => {
        expect(mapRowStatus({ ...base, mapping_status: "conflict" })).toBe(
            "CONFLICT",
        )
        expect(
            mapRowStatus({ ...base, mapping_status: "pending_mapping" }),
        ).toBe("PENDING_MAPPING")
        expect(mapRowStatus({ ...base, import_status: "failed" })).toBe(
            "FAILED",
        )
        expect(mapRowStatus({ ...base, import_status: "skipped" })).toBe(
            "SKIPPED",
        )
        expect(mapRowStatus({ ...base, parse_status: "invalid" })).toBe(
            "FAILED",
        )
    })

    it("returns null for clean rows", () => {
        expect(mapRowStatus(base)).toBeNull()
    })
})

describe("toListItem", () => {
    it("maps a backend batch item into a list row", () => {
        const row = toListItem(baseBatch, "VALIDATION")
        expect(row).toEqual({
            batchId: "b1",
            batchNo: "B-001",
            environment: "VALIDATION",
            sourceObjectSet: ["CUSTOMER", "SKU"],
            baselineDate: "2025-01-01",
            importRuleVersion: "v3",
            stage: "CONFIRM",
            status: "AWAITING_CONFIRMATION",
            progressLabel: "5/10",
            confirmationSummary: "—",
            initiatorLabel: "—",
            updatedAt: "2025-01-01T00:00:00.000Z",
        })
    })

    it("falls back to the status label when there are no rows", () => {
        const row = toListItem({ ...baseBatch, total_rows: 0 }, "VALIDATION")
        expect(row.progressLabel).toBe("待业务确认")
    })
})

describe("buildBatchView", () => {
    it("maps a pending confirmation batch with a registered work item", () => {
        const view = buildBatchView(
            baseBatch,
            [makeConfirmation()],
            "VALIDATION",
            { batchId: "b1" },
        )
        expect(view.status).toBe("AWAITING_CONFIRMATION")
        expect(view.stage).toBe("CONFIRM")
        expect(view.trialVersion).toBe("3")
        expect(view.formalDataFormed).toBe(false)
        expect(view.productionGates.workItemTypeRegistered).toBe(true)
        expect(view.confirmations).toHaveLength(1)
        expect(view.confirmations[0]).toMatchObject({
            confirmationId: "c1",
            scope: "SALES",
            result: "PENDING",
            trialVersion: "3",
        })
        expect(view.confirmations[0]!.workItem).toMatchObject({
            workItemId: "w1",
            allowedActions: ["PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"],
        })
        expect(view.allowedActions).toEqual([
            "PROCESS",
            "CONFIRM_SCOPE",
            "RETURN_FOR_FIX",
        ])
        expect(view.actionBlockers).toEqual([])
    })

    it("blocks the confirmation entry when no task is registered", () => {
        const view = buildBatchView(
            baseBatch,
            [makeConfirmation({ work_item: null })],
            "VALIDATION",
            { batchId: "b1" },
        )
        expect(view.status).toBe("CONFIRMATION_BLOCKED")
        expect(view.actionBlockers).toEqual([
            {
                action: "CONFIRM_SCOPE",
                code: "IMPORT_CONFIRMATION_TASK_MISSING",
                message:
                    "当前试算的责任确认任务不完整，请联系管理员重新生成确认任务。",
            },
        ])
    })

    it("blocks on a mismatched task entry context", () => {
        const view = buildBatchView(
            baseBatch,
            [makeConfirmation()],
            "VALIDATION",
            {
                batchId: "b1",
                workItemId: "w9",
                confirmationScope: "SALES",
                queueContextId: "q1",
            },
        )
        expect(view.status).toBe("CONFIRMATION_BLOCKED")
        expect(view.actionBlockers).toEqual([
            {
                action: "CONFIRM_SCOPE",
                code: "IMPORT_CONFIRMATION_CONTEXT_MISMATCH",
                message:
                    "任务入口与当前批次责任范围不一致，请返回待处理列表重新打开。",
            },
        ])
    })

    it("allows start/apply and cancel once everything is confirmed", () => {
        const confirmed = makeConfirmation({
            status: "CONFIRMED",
            decided_by: "李四",
            decided_at: 1735690000,
        })
        const view = buildBatchView(
            { ...baseBatch, status: "ready_to_apply" },
            [confirmed],
            "VALIDATION",
            { batchId: "b1" },
        )
        expect(view.status).toBe("READY_TO_APPLY")
        expect(view.productionGates.allConfirmationsComplete).toBe(true)
        expect(view.allowedActions).toContain("START_APPLY")
        expect(view.allowedActions).toContain("CANCEL_PENDING")
        expect(view.confirmations[0]).toMatchObject({
            result: "CONFIRMED",
            confirmedByLabel: "李四",
            confirmedAt: "2025-01-01T00:06:40.000Z",
        })
    })

    it("offers a retry for partial success with failed rows", () => {
        const view = buildBatchView(
            {
                ...baseBatch,
                status: "partial_failed",
                failed_rows: 2,
                background_job_id: "job-1",
            },
            [],
            "VALIDATION",
            { batchId: "b1" },
        )
        expect(view.status).toBe("PARTIAL_SUCCESS")
        expect(view.formalDataFormed).toBe(true)
        expect(view.allowedActions).toContain("RETRY_FAILED")
        expect(view.backgroundJob).toMatchObject({
            jobId: "job-1",
            status: "partial",
            processed: 7,
            succeeded: 5,
            failed: 2,
        })
    })

    it("drops unknown backend actions and keeps registered ones", () => {
        const withUnknown = makeConfirmation()
        withUnknown.work_item!.allowed_actions = [
            "CONFIRM_SCOPE",
            "SOME_FUTURE_ACTION",
        ]
        const view = buildBatchView(baseBatch, [withUnknown], "VALIDATION", {
            batchId: "b1",
        })
        expect(view.confirmations[0]!.workItem!.allowedActions).toEqual([
            "CONFIRM_SCOPE",
        ])
    })

    it("uses the latest non-invalidated trial version", () => {
        const older = makeConfirmation({ id: "c1", trial_version: 2 })
        const newer = makeConfirmation({
            id: "c2",
            trial_version: 5,
            status: "INVALIDATED",
        })
        const view = buildBatchView(baseBatch, [older, newer], "VALIDATION", {
            batchId: "b1",
        })
        expect(view.trialVersion).toBe("2")
    })
})

describe("backend DTO type shape", () => {
    it("accepts a minimal backend list item", () => {
        const item: BackendBatchListItem = {
            id: "b1",
            batch_no: "B-001",
            source_system_id: "sys",
            source_object_set: "",
            baseline_date: "",
            import_rule_version: "",
            status: "pending_validation",
            total_rows: 0,
            success_rows: 0,
            failed_rows: 0,
            version: 1,
            created_at: 1,
        }
        expect(toListItem(item, "PRODUCTION").batchId).toBe("b1")
    })
})
