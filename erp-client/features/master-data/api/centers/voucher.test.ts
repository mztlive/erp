import { beforeEach, describe, expect, it, vi } from "vitest"

import type { VoucherCategoryProfileDto } from "@/features/master-data/api/contracts"
import { fetchAllPages } from "@/features/master-data/api/lists"
import { centerVoucher } from "./voucher"

vi.mock("@/features/master-data/api/lists", () => ({ fetchAllPages: vi.fn() }))

const revisions: VoucherCategoryProfileDto[] = [
    {
        id: "rev-1",
        sku_id: "sku-1",
        sku_no: "VC-001",
        name: "当前名称",
        revision_no: 1,
        description: "最初描述",
        status: "active",
        created_at: 1700000000,
        version: 1,
    },
    {
        id: "other-rev",
        sku_id: "sku-2",
        sku_no: "VC-002",
        name: "另一类目",
        revision_no: 9,
        description: "其他类目描述",
        status: "active",
        created_at: 1700000000,
        version: 1,
    },
    {
        id: "rev-3",
        sku_id: "sku-1",
        sku_no: "VC-001",
        name: "当前名称",
        revision_no: 3,
        description: "最新描述",
        status: "active",
        created_at: 1700200000,
        version: 3,
    },
    {
        id: "rev-2",
        sku_id: "sku-1",
        sku_no: "VC-001",
        name: "当前名称",
        revision_no: 2,
        description: "中间描述",
        status: "disabled",
        created_at: 1700100000,
        version: 2,
    },
]

beforeEach(() => {
    vi.mocked(fetchAllPages).mockReset()
    vi.mocked(fetchAllPages).mockImplementation(async (path) =>
        path === "/admin/voucher-category-profiles" ? revisions : [],
    )
})

describe("centerVoucher", () => {
    it("keeps every revision for one SKU, newest first, with its saved description and status", async () => {
        const detail = await centerVoucher("sku-1")
        expect(detail?.currentRevision.revisionId).toBe("rev-3")
        expect(
            detail?.revisionTimeline.map((revision) => ({
                version: revision.revisionNo,
                description: revision.descriptionSnapshot,
                current: revision.isCurrent,
                status: revision.lifecycleAtRevision,
            })),
        ).toEqual([
            {
                version: 3,
                description: "最新描述",
                current: true,
                status: "ENABLED",
            },
            {
                version: 2,
                description: "中间描述",
                current: false,
                status: "DISABLED",
            },
            {
                version: 1,
                description: "最初描述",
                current: false,
                status: "ENABLED",
            },
        ])
        expect(detail?.revisionTimeline[1].nameSnapshot).toBe("")
        expect(detail?.revisionTimeline[1].effectiveFrom).toBe(
            new Date(1700100000 * 1000).toISOString(),
        )
    })

    it("resolves an old profile link to the latest detail and all sibling revisions", async () => {
        expect(await centerVoucher("rev-1")).toEqual(
            await centerVoucher("sku-1"),
        )
    })

    it("returns null for a missing category and propagates a failed history request", async () => {
        expect(await centerVoucher("missing")).toBeNull()
        vi.mocked(fetchAllPages).mockRejectedValueOnce(new Error("unavailable"))
        await expect(centerVoucher("sku-1")).rejects.toThrow("unavailable")
    })
})
