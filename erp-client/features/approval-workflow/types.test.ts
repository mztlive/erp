import { describe, expect, it } from "vitest"

import { mapUpgradeBindingResultViewDto } from "./types"

describe("mapUpgradeBindingResultViewDto", () => {
    it("maps the immutable action-backed upgrade result", () => {
        expect(
            mapUpgradeBindingResultViewDto({
                document_type: "sales_order",
                document_id: "sales-1",
                original_business_object_version: "7",
                new_binding: {
                    approval_process_definition_id: "definition-2",
                    approval_definition_version: 2,
                    approval_binding_version: "3",
                    approval_definition_bound_at: 1_725_171_200,
                },
                action_id: "action-1",
                outcome: "REPLAY",
            }),
        ).toEqual({
            documentType: "sales_order",
            documentId: "sales-1",
            originalBusinessObjectVersion: "7",
            newBinding: {
                approvalProcessDefinitionId: "definition-2",
                approvalDefinitionVersion: 2,
                approvalBindingVersion: "3",
                approvalDefinitionBoundAt: 1_725_171_200,
            },
            actionId: "action-1",
            outcome: "REPLAY",
        })
    })
})
