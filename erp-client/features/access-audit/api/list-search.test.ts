import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchAccessList } from "./list"
import type { BackendAuditEvent } from "./backend-types"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())

function mockPages(total: number) {
    vi.mocked(apiGet).mockImplementation(async (path, params) => {
        if (path === "/admin/roles" || path === "/admin/admins")
            return [] as never
        if (path !== "/admin/audit-events")
            return { items: [], total: 0 } as never
        const page = Number(params?.page ?? 1)
        const pageSize = Number(params?.page_size ?? 100)
        const rows: BackendAuditEvent[] = Array.from(
            { length: Math.min(pageSize, total - (page - 1) * pageSize) },
            (_, i) => ({
                id: `event-${(page - 1) * pageSize + i}`,
                actor_id: "u1",
                actor_label: "操作员",
                actor_role: "role",
                action_type: "sales_order.create",
                object_type: "sales_order",
                result: "SUCCESS",
                changed_field_names: [],
                created_at: 1700000000,
            }),
        )
        return { items: rows, total } as never
    })
}

it("审计搜索与日期、追踪号进入后端，跨页保留服务端命中", async () => {
    mockPages(101)
    const result = await fetchAccessList({
        view: "audit",
        q: " 新建 ",
        from: "2026-09-01",
        to: "2026-09-01",
        traceId: "trace",
    })
    expect(result.auditEvents).toHaveLength(101)
    expect(result.metrics.auditEventCount).toBe(101)
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/audit-events",
        expect.objectContaining({
            q: "新建",
            trace_id: "trace",
            keyword_actions: expect.stringContaining("sales_order.create"),
            created_from: Date.parse("2026-09-01T00:00:00+08:00") / 1000,
            created_before: Date.parse("2026-09-02T00:00:00+08:00") / 1000,
            page: 2,
        }),
    )
})

it("角色页仅取审计计数所需的一页", async () => {
    mockPages(101)
    const result = await fetchAccessList({ view: "roles" })
    expect(result.metrics.auditEventCount).toBe(101)
    const calls = vi
        .mocked(apiGet)
        .mock.calls.filter(([path]) => path === "/admin/audit-events")
    expect(calls).toHaveLength(1)
    expect(calls[0][1]).toEqual(expect.objectContaining({ page_size: 1 }))
})
