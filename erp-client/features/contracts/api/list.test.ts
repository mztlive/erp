import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchContracts } from "./list"
vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())

it("合同查询传递已生效条件和真实页码，并保留服务端总数及全范围候选", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        items: [
            {
                id: "contract-101",
                contract_no: "HT-101",
                customer_id: "customer-1",
                settlement_party_id: "party-1",
                status: "EFFECTIVE",
                created_at: 1,
                current_revision: {
                    customer_name: "命中客户",
                    settlement_party_name: "结算主体",
                    valid_from: "2026-01-01",
                    valid_to: "2027-01-01",
                    revision_no: 1,
                },
            },
        ],
        total: 101,
        metrics: {
            all: 500,
            effective: 300,
            expiring_30d: 7,
            expired: 100,
            terminated: 100,
        },
        settlement_options: [
            { value: "party-outside-page", label: "其他结算主体" },
        ],
        owner_options: [{ value: "user-1", label: "负责人甲" }],
        scope_version: "v1",
        policy_version: 3,
        organization_version: 4,
        scope_summary:
            "合同当前客户主负责人、协作关系、负责人所属组织及合法单据参与",
        as_of: "2026-09-14T08:00:00Z",
        ownership_basis: "current_customer_owner",
    } as never)
    const result = await fetchContracts({
        q: " 命中客户 ",
        page: 6,
        pageSize: 20,
        metric: "expiring_30d",
        settlementPartyId: "party-1",
        ownerUserIds: "user-1",
        orgUnitIds: "org-1",
        includeDescendants: true,
        scopeVersion: "v1",
        sort: "validity",
        dir: "asc",
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/contracts",
        expect.objectContaining({
            q: "命中客户",
            page: 6,
            page_size: 20,
            metric: "expiring_30d",
            settlement_party_id: "party-1",
            owner_user_ids: "user-1",
            org_unit_ids: "org-1",
            include_descendants: true,
            scope_version: "v1",
            sort_by: "validity",
        }),
    )
    expect(result.items[0].contractId).toBe("contract-101")
    expect(result.total).toBe(101)
    expect(result.metrics.all).toBe(500)
    expect(result.settlementOptions[0].value).toBe("party-outside-page")
    expect(result.emptyReason).toBeUndefined()
    expect(result.scopeVersion).toBe("v1")
    expect(result.ownershipBasis).toBe("current_customer_owner")
})

it("空关键词清除后仍用真实第一页，默认采用将到期优先排序", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        items: [],
        total: 0,
        metrics: {
            all: 0,
            effective: 0,
            expiring_30d: 0,
            expired: 0,
            terminated: 0,
        },
        settlement_options: [],
        owner_options: [],
        empty_reason: "no_scope",
        scope_version: "v2",
        policy_version: 1,
        organization_version: 1,
        scope_summary:
            "合同当前客户主负责人、协作关系、负责人所属组织及合法单据参与",
        as_of: "2026-09-14T08:00:00Z",
        ownership_basis: "current_customer_owner",
    } as never)
    const result = await fetchContracts({
        q: "  ",
        page: 1,
        pageSize: 20,
        metric: "all",
        includeDescendants: false,
    })
    expect(result.emptyReason).toBe("no_scope")
    expect(result.scopeVersion).toBe("v2")
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/contracts",
        expect.objectContaining({
            q: undefined,
            page: 1,
            sort_by: "expiry_priority",
        }),
    )
})
