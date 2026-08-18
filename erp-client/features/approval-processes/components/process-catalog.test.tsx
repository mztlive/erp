import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { catalogFixture } from "../fixtures"
import { DOCUMENT_TYPES, NO_APPROVAL_DOCUMENT_TYPES } from "../types"
import { ProcessCatalog, visibleActions } from "./process-catalog"

describe("process catalog", () => {
    it("renders all 20 fixed document types including VoucherSalesOrder", () => {
        render(
            <ProcessCatalog
                items={catalogFixture({
                    stock_adjustment: {
                        configuration_status: "PUBLISHED",
                        published_version: "2",
                        allowed_actions: ["CREATE_DRAFT", "RETIRE"],
                    },
                    sales_order: {
                        configuration_status: "MISSING_CONFIGURATION",
                        draft_version: "1",
                        allowed_actions: ["REPLACE_NODES", "PUBLISH"],
                    },
                })}
                permissions={["*:*"]}
                onCreateDraft={vi.fn()}
                onContinueDraft={vi.fn()}
            />,
        )
        for (const documentType of DOCUMENT_TYPES) {
            expect(
                document.querySelector(
                    `[data-document-type="${documentType}"]`,
                ),
            ).not.toBeNull()
        }
        expect(screen.getByText("卡券销售单")).toBeTruthy()
        expect(screen.getByText("销售单（实物及服务）")).toBeTruthy()
        expect(DOCUMENT_TYPES).toHaveLength(20)
    })

    it("gives 8 NO_APPROVAL types no write entry", () => {
        const items = catalogFixture()
        for (const documentType of NO_APPROVAL_DOCUMENT_TYPES) {
            const item = items.find((row) => row.document_type === documentType)
            expect(item).toBeTruthy()
            expect(visibleActions(item!, ["*:*"])).toEqual([])
        }
        render(
            <ProcessCatalog
                items={items}
                permissions={["*:*"]}
                onCreateDraft={vi.fn()}
                onContinueDraft={vi.fn()}
            />,
        )
        const invoiceRow = document.querySelector(
            '[data-document-type="invoice"]',
        )
        expect(invoiceRow?.textContent).toContain("无需审批")
        expect(invoiceRow?.textContent).not.toContain("新建草稿")
        expect(invoiceRow?.textContent).not.toContain("继续编辑")
        expect(invoiceRow?.textContent).not.toContain("发布")
        expect(invoiceRow?.textContent).not.toContain("退役")
    })

    it("shows PROCESS_REQUIRED missing configuration as a blocker", () => {
        render(
            <ProcessCatalog
                items={catalogFixture()}
                permissions={["*:*"]}
                onCreateDraft={vi.fn()}
                onContinueDraft={vi.fn()}
            />,
        )
        const salesRow = document.querySelector(
            '[data-document-type="sales_order"]',
        )
        expect(salesRow?.getAttribute("data-blocked")).toBe("true")
        expect(salesRow?.textContent).toContain("配置缺失")
        expect(salesRow?.textContent).not.toContain("无需审批 / 不适用")
    })

    it("hides actions when permission or allowed_actions is missing", () => {
        const item = catalogFixture({
            stock_adjustment: {
                allowed_actions: ["CREATE_DRAFT"],
            },
        }).find((row) => row.document_type === "stock_adjustment")!
        expect(visibleActions(item, ["approval_process:read"])).toEqual([])
        expect(visibleActions(item, ["approval_process:create"])).toEqual([
            "CREATE_DRAFT",
        ])
        expect(
            visibleActions({ ...item, allowed_actions: [] }, [
                "approval_process:create",
            ]),
        ).toEqual([])
    })
})
