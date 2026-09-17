import { beforeEach, expect, it, vi } from "vitest"

const apiGet = vi.fn()
vi.mock("@/lib/api", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
}))

import { fetchIntegrationQueue } from "./queue-requests"

beforeEach(() => {
    apiGet.mockReset()
})

it("把稳定处理人 ID 交给服务端，不再发送 me 或拉全部分页", async () => {
    apiGet.mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 50,
        empty_reason: "no_scope",
        scope_version: "v1",
        scope_summary: "集成当前处理人所属内部组织范围",
        ownership_basis: "current_handler",
    })
    const result = await fetchIntegrationQueue({
        view: "result_unknown",
        mode: "errors",
        environment: "production",
        currentUserId: "user-42",
        queueContextId: "queue:W29:result_unknown",
    })
    expect(apiGet).toHaveBeenCalledTimes(1)
    const params = apiGet.mock.calls[0]?.[1] as Record<string, unknown>
    expect(params.handler_user_ids).toBeUndefined()
    expect(params.owner_user_id).toBeUndefined()
    expect(params.error_class).toBe("result_unknown")
    expect(result.emptyReason).toBe("no_scope")
    expect(result.scopeVersion).toBe("v1")
})

it("拒绝把 me 当作人员 ID，显式 handlerUserIds 优先生效", async () => {
    apiGet.mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 50,
    })
    await fetchIntegrationQueue({
        view: "mine",
        mode: "errors",
        environment: "production",
        handlerUserIds: "user-7,user-8",
        currentUserId: "user-42",
        operatorUserIds: "user-9",
    })
    expect(apiGet).toHaveBeenCalledTimes(1)
    const params = apiGet.mock.calls[0]?.[1] as Record<string, unknown>
    expect(params.handler_user_ids).toBe("user-7,user-8")
    expect(params.operator_user_ids).toBe("user-9")
    expect(params.handler_user_ids).not.toBe("me")
})
