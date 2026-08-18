import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet, apiPost, apiPut } from "@/lib/api"
import { createApiError } from "@/lib/api/errors"

import {
    createDefinitionDraft,
    fetchDefinitionCatalog,
    fetchEligibleAssignees,
    publishDefinition,
    replaceDefinitionNodes,
} from "./api"
import { catalogFixture, detailFixture } from "./fixtures"
import { unwrapResult } from "./result"
import {
    buildCreateDraftRequest,
    buildLockRequest,
    buildReplaceNodesRequest,
} from "./write-payload"
import { seedDraftNodes } from "./draft-nodes"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    apiPut: vi.fn(),
}))

const mockedGet = vi.mocked(apiGet)
const mockedPost = vi.mocked(apiPost)
const mockedPut = vi.mocked(apiPut)

beforeEach(() => {
    vi.clearAllMocks()
})

describe("approval process api", () => {
    it("returns catalog through ResultAsync and keeps versions as strings", async () => {
        mockedGet.mockResolvedValue(
            catalogFixture().map((item) => ({
                ...item,
                published_version: item.published_version
                    ? Number(item.published_version)
                    : null,
            })),
        )
        const result = await fetchDefinitionCatalog()
        expect(result.ok).toBe(true)
        if (!result.ok) return
        expect(result.value).toHaveLength(20)
        expect(
            result.value.some(
                (item) => item.document_type === "voucher_sales_order",
            ),
        ).toBe(true)
        expect(
            result.value.every(
                (item) =>
                    item.published_version == null ||
                    typeof item.published_version === "string",
            ),
        ).toBe(true)
        expect(mockedGet).toHaveBeenCalledWith(
            "/admin/approval-processes/catalog",
        )
    })

    it("maps request failures to ResultAsync error without throwing", async () => {
        mockedGet.mockRejectedValue(
            createApiError({
                kind: "Http",
                status: 403,
                code: "FORBIDDEN",
                message: "Permission denied",
            }),
        )
        const result = await fetchDefinitionCatalog()
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error.status).toBe(403)
        expect(() => unwrapResult(result)).toThrow()
    })

    it("create draft request has no source definition id", async () => {
        mockedPost.mockResolvedValue(detailFixture())
        const request = buildCreateDraftRequest(
            "stock_adjustment",
            "库存调整审批",
            "EMPTY",
            "k1",
        )
        expect(request).toEqual({
            document_type: "stock_adjustment",
            name: "库存调整审批",
            draft_source: "EMPTY",
            idempotency_key: "k1",
        })
        expect(JSON.stringify(request)).not.toContain("source_definition_id")
        await createDefinitionDraft(request)
        expect(mockedPost).toHaveBeenCalledWith(
            "/admin/approval-process-definitions/drafts",
            request,
        )
    })

    it("replace nodes omits key purpose transition role handler and action", async () => {
        mockedPut.mockResolvedValue(detailFixture())
        const nodes = seedDraftNodes("stock_adjustment", [
            {
                node_id: "n1",
                node_key: "server-key",
                node_name: "仓储复核",
                node_type: "USER_APPROVAL",
                node_purpose: null,
                display_order: 1,
                assignee_user_id: "user-zhang",
                assignee_name_snapshot: "张三",
            },
            {
                node_id: "",
                node_key: "",
                node_name: "财务复核",
                node_type: "USER_APPROVAL",
                node_purpose: null,
                display_order: 2,
                assignee_user_id: "user-li",
                assignee_name_snapshot: "李四",
            },
        ])
        nodes[1] = { ...nodes[1]!, node_id: null }
        const request = buildReplaceNodesRequest("3", nodes)
        expect(request.expected_definition_lock_version).toBe("3")
        expect(request.nodes[0]).toEqual({
            node_id: "n1",
            node_name: "仓储复核",
            display_order: 1,
            assignee_user_id: "user-zhang",
        })
        expect(request.nodes[1]).toEqual({
            node_name: "财务复核",
            display_order: 2,
            assignee_user_id: "user-li",
        })
        const serialized = JSON.stringify(request)
        expect(serialized).not.toContain("node_key")
        expect(serialized).not.toContain("node_purpose")
        expect(serialized).not.toContain("transitions")
        expect(serialized).not.toContain("role")
        expect(serialized).not.toContain("handler")
        expect(serialized).not.toContain("action")
        await replaceDefinitionNodes("def-1", request)
        expect(mockedPut).toHaveBeenCalledWith(
            "/admin/approval-process-definitions/def-1/nodes",
            request,
        )
    })

    it("publish carries lock version as string and a new idempotency key", async () => {
        mockedPost.mockResolvedValue(detailFixture({ status: "PUBLISHED" }))
        const request = buildLockRequest("7", "publish:abc")
        await publishDefinition("def-1", request)
        expect(mockedPost).toHaveBeenCalledWith(
            "/admin/approval-process-definitions/def-1/publish",
            {
                expected_definition_lock_version: "7",
                idempotency_key: "publish:abc",
            },
        )
    })

    it("eligible assignees query goes to the server with search", async () => {
        mockedGet.mockResolvedValue([{ user_id: "u1", name: "张三" }])
        const result = await fetchEligibleAssignees("sales_order", "张")
        expect(result.ok).toBe(true)
        expect(mockedGet).toHaveBeenCalledWith(
            "/admin/approval-processes/sales_order/eligible-assignees",
            { search: "张", limit: 20 },
        )
    })
})
