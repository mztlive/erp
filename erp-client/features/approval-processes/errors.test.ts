import { describe, expect, it } from "vitest"

import { createApiError } from "@/lib/api/errors"

import { definitionErrorMessage, isDefinitionVersionConflict } from "./errors"

describe("definition errors", () => {
    it("maps stable codes instead of backend message text", () => {
        expect(
            definitionErrorMessage(
                createApiError({
                    kind: "Http",
                    status: 409,
                    code: "APPROVAL_DEFINITION_VERSION_CONFLICT",
                    message: "definition lock version mismatch",
                }),
            ),
        ).toBe("审批流程已被更新，请核对当前版本后重新确认。")
        expect(
            definitionErrorMessage(
                createApiError({
                    kind: "Validation",
                    status: 422,
                    code: "APPROVAL_DEFINITION_INVALID",
                    message: "graph invariant failed at node X",
                }),
            ),
        ).toBe("审批流程未通过检查，请核对节点和审批人。")
        expect(
            definitionErrorMessage(
                createApiError({
                    kind: "Unknown",
                    message: "internal boom",
                    requestId: "corr-99",
                }),
            ),
        ).toBe("系统暂时无法完成操作。错误编号 corr-99")
    })

    it("detects lock conflicts only by status and code", () => {
        expect(
            isDefinitionVersionConflict(
                createApiError({
                    kind: "Http",
                    status: 409,
                    code: "APPROVAL_DEFINITION_VERSION_CONFLICT",
                    message: "anything",
                }),
            ),
        ).toBe(true)
        expect(
            isDefinitionVersionConflict(
                createApiError({
                    kind: "Http",
                    status: 409,
                    code: "APPROVAL_PROCESS_NOT_CONFIGURED",
                    message: "APPROVAL_DEFINITION_VERSION_CONFLICT",
                }),
            ),
        ).toBe(false)
    })
})
