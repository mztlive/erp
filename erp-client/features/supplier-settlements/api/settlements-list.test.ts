import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

import { apiGet } from "@/lib/api"
import { fetchSettlementList } from "./settlements-list"

const apiGetMock = vi.mocked(apiGet)

describe("fetchSettlementList data scope", () => {
    beforeEach(() => {
        apiGetMock.mockReset()
    })

    it("sends three-role filters and maps no_scope empty reason", async () => {
        apiGetMock.mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 50,
            stats: {
                pending_reconciliation_count: 0,
                has_difference_count: 0,
                pending_review_count: 0,
                confirmed_amount: "0.00",
            },
            processing_state: "EMPTY",
            empty_reason: "no_scope",
            scope_version: "v1",
        })
        const view = await fetchSettlementList({
            view: "pending",
            ownerUserIds: "owner-1",
            operatorUserIds: "op-1",
            handlerUserIds: "reviewer-1",
            orgUnitIds: "org-a",
            page: 1,
        })
        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/supplier-settlement-statements",
            expect.objectContaining({
                owner_user_ids: "owner-1",
                operator_user_ids: "op-1",
                handler_user_ids: "reviewer-1",
                org_unit_ids: "org-a",
            }),
        )
        expect(view.emptyReason).toBe("NO_SCOPE")
        expect(view.hasModulePermission).toBe(true)
        expect(view.scopeVersion).toBe("v1")
        expect(
            (view as { hasDataScope?: boolean }).hasDataScope,
        ).toBeUndefined()
    })

    it("uses current user id for prepared_by_me instead of me", async () => {
        apiGetMock.mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 50,
            stats: {
                pending_reconciliation_count: 0,
                has_difference_count: 0,
                pending_review_count: 0,
                confirmed_amount: "0.00",
            },
            processing_state: "EMPTY",
        })
        await fetchSettlementList({
            view: "prepared_by_me",
            currentUserId: "user-9",
            page: 1,
        })
        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/supplier-settlement-statements",
            expect.objectContaining({ owner_user_ids: "user-9" }),
        )
        expect(apiGetMock.mock.calls[0]?.[1]).not.toEqual(
            expect.objectContaining({ owner_user_ids: "me" }),
        )
    })

    it("maps review_by_me to handler_user_ids of the current user", async () => {
        apiGetMock.mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 50,
            stats: {
                pending_reconciliation_count: 0,
                has_difference_count: 0,
                pending_review_count: 0,
                confirmed_amount: "0.00",
            },
            processing_state: "EMPTY",
        })
        await fetchSettlementList({
            view: "review_by_me",
            currentUserId: "user-9",
            page: 2,
            scopeVersion: "v-scope",
        })
        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/supplier-settlement-statements",
            expect.objectContaining({
                handler_user_ids: "user-9",
                scope_version: "v-scope",
            }),
        )
    })
})
