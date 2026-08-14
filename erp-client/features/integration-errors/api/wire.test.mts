import assert from "node:assert/strict"
import test from "node:test"

// Node 24 executes TypeScript via type stripping; the production compiler uses
// extensionless imports, so this explicit extension is intentionally test-only.
import {
    mapAllowedIntegrationActions,
    mapBackendEvidenceRefs,
    mapBackendReconciliationReasonRegistry,
    mapBackendResolutionEvidencePolicy,
    toDirectReconciliationWire,
    toTaskActionWire,
    toTaskCompletionWire,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./wire.ts"

const evidence = {
    kind: "COMPENSATION_RESULT" as const,
    recordId: "customer_refund:refund-1",
    label: "客户退款已过账",
}

test("task action serializes nested evidence refs as snake_case", () => {
    assert.deepEqual(
        toTaskActionWire({
            itemType: "ERROR_TASK",
            itemId: "task-1",
            workItemId: "work-1",
            expectedSubjectVersion: "3",
            expectedTaskVersion: "5",
            kind: "ADD_EVIDENCE",
            operationId: "operation-1",
            idempotencyKey: "idempotency-1",
            evidenceRefs: [evidence],
        }),
        {
            work_item_id: "work-1",
            expected_task_version: "5",
            expected_subject_version: "3",
            action: {
                item_type: "ERROR_TASK",
                item_id: "task-1",
                kind: "ADD_EVIDENCE",
                operation_id: "operation-1",
                evidence_refs: [
                    {
                        kind: "COMPENSATION_RESULT",
                        record_id: "customer_refund:refund-1",
                        label: "客户退款已过账",
                    },
                ],
            },
            idempotency_key: "idempotency-1",
        },
    )
})

test("task completion serializes policy key and evidence refs as snake_case", () => {
    const body = toTaskCompletionWire({
        itemType: "RECONCILIATION_DIFFERENCE",
        itemId: "difference-1",
        workItemId: "work-2",
        expectedSubjectVersion: "7",
        expectedTaskVersion: "9",
        operationId: "operation-2",
        idempotencyKey: "idempotency-2",
        reasonCode: "TERMINAL_EVIDENCE_VERIFIED",
        evidencePolicyId: "w29-financial-difference",
        evidencePolicyVersion: 1,
        policyKey: {
            errorType: "amount_mismatch",
            fundsImpact: "POTENTIAL",
        },
        evidenceRefs: [evidence],
    })

    assert.deepEqual(body.decision.policy_key, {
        error_type: "amount_mismatch",
        funds_impact: "POTENTIAL",
    })
    assert.equal(body.decision.evidence_refs[0]?.record_id, evidence.recordId)
    assert.equal("recordId" in body.decision.evidence_refs[0]!, false)
})

test("direct terminal decision serializes registry identity as snake_case", () => {
    const body = toDirectReconciliationWire({
        differenceId: "difference-2",
        expectedDifferenceVersion: "2",
        operationId: "operation-3",
        idempotencyKey: "idempotency-3",
        decision: {
            kind: "TERMINAL_CONCLUSION",
            reasonCode: "COMPENSATION_CLOSED",
            reasonRegistryId: "w29-reconciliation-reasons",
            reasonRegistryVersion: 1,
            registeredReasonId: "COMPENSATION_CLOSED",
            conclusion: "CONFIRM_VALID_DIFFERENCE",
            evidenceRefs: [evidence],
        },
    })

    assert.deepEqual(body.decision, {
        kind: "TERMINAL_CONCLUSION",
        reason_code: "COMPENSATION_CLOSED",
        reason_registry_id: "w29-reconciliation-reasons",
        reason_registry_version: 1,
        registered_reason_id: "COMPENSATION_CLOSED",
        conclusion: "CONFIRM_VALID_DIFFERENCE",
        evidence_refs: [
            {
                kind: "COMPENSATION_RESULT",
                record_id: "customer_refund:refund-1",
                label: "客户退款已过账",
            },
        ],
    })
})

test("direct terminal decision rejects a reason outside the selected registry entry", () => {
    assert.throws(
        () =>
            toDirectReconciliationWire({
                differenceId: "difference-2",
                expectedDifferenceVersion: "2",
                operationId: "operation-3",
                idempotencyKey: "idempotency-3",
                decision: {
                    kind: "TERMINAL_CONCLUSION",
                    reasonCode: "BUSINESS_CONFIRMED_NO_ERROR",
                    reasonRegistryId: "w29-reconciliation-reasons",
                    reasonRegistryVersion: 1,
                    registeredReasonId: "COMPENSATION_CLOSED",
                    conclusion: "CONFIRM_VALID_DIFFERENCE",
                    evidenceRefs: [evidence],
                },
            }),
        /必须一致/,
    )
})

test("detail registry and policy mapping fail closed on malformed contracts", () => {
    assert.deepEqual(
        mapBackendResolutionEvidencePolicy({
            evidence_policy_id: "w29-policy",
            evidence_policy_version: 2,
            key: {
                error_type: "amount_mismatch",
                funds_impact: "POSTED",
            },
            required_evidence_kinds: ["FINANCIAL_RECONCILIATION"],
            reviewer_separation: "DISTINCT_FINANCE_REVIEWER",
        }),
        {
            evidencePolicyId: "w29-policy",
            evidencePolicyVersion: 2,
            key: {
                errorType: "amount_mismatch",
                fundsImpact: "POSTED",
            },
            requiredEvidenceKinds: ["FINANCIAL_RECONCILIATION"],
            reviewerSeparation: "DISTINCT_FINANCE_REVIEWER",
        },
    )
    assert.equal(
        mapBackendResolutionEvidencePolicy({
            evidence_policy_id: "w29-policy",
            evidence_policy_version: 2,
            key: { error_type: "amount_mismatch", funds_impact: "UNKNOWN" },
            required_evidence_kinds: ["FINANCIAL_RECONCILIATION"],
            reviewer_separation: "DISTINCT_FINANCE_REVIEWER",
        }),
        undefined,
    )
    assert.equal(
        mapBackendResolutionEvidencePolicy({
            evidence_policy_id: "w29-policy",
            evidence_policy_version: 2,
            key: { error_type: "amount_mismatch", funds_impact: "POSTED" },
            required_evidence_kinds: [],
            reviewer_separation: "DISTINCT_FINANCE_REVIEWER",
        }),
        undefined,
    )
    assert.equal(
        mapBackendReconciliationReasonRegistry({
            reason_registry_id: "w29-reasons",
            reason_registry_version: 1,
            registered_reasons: [
                {
                    registered_reason_id: "COMPENSATION_CLOSED",
                    registered_reason_version: 1,
                    conclusion: "UNREGISTERED_CONCLUSION",
                    label: "已补偿闭环",
                    required_evidence_kinds: ["COMPENSATION_RESULT"],
                },
            ],
        }),
        undefined,
    )
    assert.equal(
        mapBackendReconciliationReasonRegistry({
            reason_registry_id: "w29-reasons",
            reason_registry_version: 1,
            registered_reasons: [
                {
                    registered_reason_id: "COMPENSATION_CLOSED",
                    registered_reason_version: 1,
                    conclusion: "CONFIRM_NO_ERROR",
                    label: "已补偿闭环",
                    required_evidence_kinds: ["COMPENSATION_RESULT"],
                },
            ],
        }),
        undefined,
    )
})

test("detail evidence and reason registry map snake_case projections", () => {
    assert.deepEqual(
        mapBackendEvidenceRefs([
            {
                kind: "EXTERNAL_CASE_RESULT",
                record_id: "inbox_message:message-1",
                label: "消息已处理",
            },
        ]),
        [
            {
                kind: "EXTERNAL_CASE_RESULT",
                recordId: "inbox_message:message-1",
                label: "消息已处理",
            },
        ],
    )
    assert.deepEqual(
        mapBackendReconciliationReasonRegistry({
            reason_registry_id: "w29-reasons",
            reason_registry_version: 1,
            registered_reasons: [
                {
                    registered_reason_id: "SOURCE_CORRECTED_AND_REATTRIBUTED",
                    registered_reason_version: 1,
                    conclusion: "CONFIRM_VALID_DIFFERENCE",
                    label: "来源已更正并重新归集",
                    required_evidence_kinds: ["BUSINESS_OBJECT_VERIFICATION"],
                },
                {
                    registered_reason_id: "BUSINESS_CONFIRMED_NO_ERROR",
                    registered_reason_version: 1,
                    conclusion: "CONFIRM_NO_ERROR",
                    label: "业务确认无误",
                    required_evidence_kinds: ["BUSINESS_OBJECT_VERIFICATION"],
                },
                {
                    registered_reason_id: "COMPENSATION_CLOSED",
                    registered_reason_version: 1,
                    conclusion: "CONFIRM_VALID_DIFFERENCE",
                    label: "补偿已闭环",
                    required_evidence_kinds: ["COMPENSATION_RESULT"],
                },
            ],
        }),
        {
            reasonRegistryId: "w29-reasons",
            reasonRegistryVersion: 1,
            registeredReasons: [
                {
                    registeredReasonId: "SOURCE_CORRECTED_AND_REATTRIBUTED",
                    registeredReasonVersion: 1,
                    conclusion: "CONFIRM_VALID_DIFFERENCE",
                    label: "来源已更正并重新归集",
                    requiredEvidenceKinds: ["BUSINESS_OBJECT_VERIFICATION"],
                },
                {
                    registeredReasonId: "BUSINESS_CONFIRMED_NO_ERROR",
                    registeredReasonVersion: 1,
                    conclusion: "CONFIRM_NO_ERROR",
                    label: "业务确认无误",
                    requiredEvidenceKinds: ["BUSINESS_OBJECT_VERIFICATION"],
                },
                {
                    registeredReasonId: "COMPENSATION_CLOSED",
                    registeredReasonVersion: 1,
                    conclusion: "CONFIRM_VALID_DIFFERENCE",
                    label: "补偿已闭环",
                    requiredEvidenceKinds: ["COMPENSATION_RESULT"],
                },
            ],
        },
    )
})

test("allowed actions fail closed without the required server strategy", () => {
    assert.deepEqual(
        mapAllowedIntegrationActions(
            ["ADD_EVIDENCE", "RESOLVE", "UNKNOWN_ACTION"],
            {
                hasWorkItem: true,
                hasResolutionPolicy: false,
                directConclusions: [],
            },
        ),
        ["ADD_EVIDENCE"],
    )
    assert.deepEqual(
        mapAllowedIntegrationActions(
            [
                "QUERY_ORIGINAL_RESULT",
                "RESOLVE",
                "CONFIRM_NO_ERROR",
                "CONFIRM_VALID_DIFFERENCE",
            ],
            {
                hasWorkItem: false,
                hasResolutionPolicy: false,
                directConclusions: ["CONFIRM_NO_ERROR"],
            },
        ),
        ["QUERY_ORIGINAL_RESULT", "CONFIRM_NO_ERROR"],
    )
})
