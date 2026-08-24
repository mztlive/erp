import { describe, expect, it } from "vitest"

import { fromHttpResponse, getErrorPresentation } from "@/lib/api/errors"

describe("error presentation safety", () => {
    it("replaces technical backend messages with an actionable explanation", () => {
        const error = fromHttpResponse(500, {
            errorMessage: "MongoServerError: E11000 duplicate key",
            requestId: "req-technical",
            success: false,
        })

        expect(getErrorPresentation(error)).toEqual({
            kind: "system",
            title: "系统暂时无法完成操作",
            description:
                "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员。",
            code: undefined,
            fieldErrors: undefined,
            requestId: "req-technical",
            retryable: true,
        })
    })

    it("does not expose an unknown English exception", () => {
        expect(
            getErrorPresentation(
                new Error("Failed to fetch dashboard"),
                "工作台加载失败，请刷新后重试。",
            ),
        ).toEqual({
            kind: "system",
            title: "系统暂时无法完成操作",
            description: "工作台加载失败，请刷新后重试。",
            retryable: true,
        })
    })
})
