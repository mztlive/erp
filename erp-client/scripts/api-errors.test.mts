import assert from "node:assert/strict"
import test from "node:test"

import {
    fromFetchError,
    fromHttpResponse,
    getErrorPresentation,
} from "../lib/api/errors.ts"

test("403 is a real Error with a user-readable permission reason", () => {
    const error = fromHttpResponse(403, {
        status: 403,
        errorMessage: "Permission denied",
        success: false,
    })

    assert.ok(error instanceof Error)
    assert.equal(error.status, 403)
    assert.deepEqual(getErrorPresentation(error), {
        kind: "permission",
        title: "权限不足",
        description:
            "当前账号没有执行此操作的权限，请联系管理员或有权限的同事。",
        code: undefined,
        requestId: undefined,
        retryable: false,
    })
})

test("specific backend validation reason is preserved", () => {
    const error = fromHttpResponse(422, {
        status: 422,
        errorMessage: "至少需要一条商品明细",
        code: "PRODUCT_LINE_REQUIRED",
        requestId: "req-1",
        success: false,
    })

    assert.ok(error instanceof Error)
    assert.deepEqual(getErrorPresentation(error), {
        kind: "validation",
        title: "提交内容未通过检查",
        description: "至少需要一条商品明细",
        code: "PRODUCT_LINE_REQUIRED",
        requestId: "req-1",
        retryable: false,
    })
})

test("network failures provide a retryable next step", () => {
    const error = fromFetchError(new TypeError("fetch failed"))
    const presentation = getErrorPresentation(error)

    assert.ok(error instanceof Error)
    assert.equal(presentation.kind, "system")
    assert.equal(presentation.title, "网络连接失败")
    assert.equal(presentation.retryable, true)
})

test("technical backend messages are replaced with an actionable explanation", () => {
    const error = fromHttpResponse(500, {
        errorMessage: "MongoServerError: E11000 duplicate key",
        requestId: "req-technical",
    })
    const presentation = getErrorPresentation(error)

    assert.equal(presentation.title, "系统暂时无法完成操作")
    assert.equal(
        presentation.description,
        "系统暂时无法完成请求，请稍后重试；如仍失败，请联系支持人员。",
    )
    assert.equal(presentation.requestId, "req-technical")
    assert.equal(presentation.retryable, true)
})

test("unknown English errors never leak directly to users", () => {
    const presentation = getErrorPresentation(
        new Error("Failed to fetch dashboard"),
        "工作台加载失败，请刷新后重试。",
    )

    assert.deepEqual(presentation, {
        kind: "system",
        title: "系统暂时无法完成操作",
        description: "工作台加载失败，请刷新后重试。",
        retryable: true,
    })
})
