import { beforeEach, expect, test, vi } from "vitest"
import { fetchAccountPermissionScopes } from "./account-permission-scopes"
import { apiGet } from "@/lib/api"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())

test("读取账号和角色范围的全部分页，并保留来源", async () => {
    vi.mocked(apiGet).mockImplementation(async (_path, params) => ({
        items: [
            {
                id: `${params?.subject_id}-${params?.page}`,
                subject_id: params?.subject_id,
                subject_type: params?.subject_type,
                scope_type: "company",
                scope_targets: [],
            },
        ],
        page: params?.page,
        page_size: 1,
        total: params?.subject_type === "user" ? 2 : 1,
    }))
    const scopes = await fetchAccountPermissionScopes("u1", ["r1", "r1"])
    expect(scopes.map((scope) => scope.id)).toEqual(["u1-1", "u1-2", "r1-1"])
    expect(apiGet).toHaveBeenCalledTimes(3)
})

test("范围读取失败不降级成无范围，分页缺失也不能静默截断", async () => {
    vi.mocked(apiGet).mockRejectedValueOnce(new Error("无权限"))
    await expect(fetchAccountPermissionScopes("u1", [])).rejects.toThrow(
        "无权限",
    )
    vi.mocked(apiGet).mockResolvedValueOnce({
        items: [],
        total: 1,
        page: 1,
        page_size: 50,
    })
    await expect(fetchAccountPermissionScopes("u1", [])).rejects.toThrow(
        "未完整返回",
    )
})
