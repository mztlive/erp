import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { seedDraftNodes } from "../draft-nodes"
import { salesOrderSavedDraft } from "../fixtures"
import { SALES_ORDER_PROCUREMENT_PURPOSE } from "../types"
import { NodeListEditor } from "./node-list-editor"
import type { EditorNode } from "../types"

vi.mock("./assignee-combobox", () => ({
    AssigneeCombobox: ({
        value,
        onChange,
        disabled,
    }: {
        value: string
        onChange: (next: { user_id: string; name: string } | null) => void
        disabled?: boolean
    }) => (
        <button
            type="button"
            disabled={disabled}
            onClick={() => onChange({ user_id: "user-picked", name: "赵六" })}
        >
            {value || "选择审批人"}
        </button>
    ),
}))

const renderEditor = (
    nodes: EditorNode[],
    onChange = vi.fn(),
    documentType: "sales_order" | "stock_adjustment" = "stock_adjustment",
) => {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    render(
        <QueryClientProvider client={client}>
            <NodeListEditor
                documentType={documentType}
                nodes={nodes}
                readOnly={false}
                onChange={onChange}
            />
        </QueryClientProvider>,
    )
    return onChange
}

describe("node list editor", () => {
    afterEach(() => {
        cleanup()
    })

    it("adds removes reorders and selects a single assignee", () => {
        const nodes: EditorNode[] = [
            {
                client_id: "a",
                node_id: "n1",
                node_name: "仓储复核",
                assignee_user_id: "user-zhang",
                assignee_name: "张三",
                node_purpose: null,
                unsaved_purpose_slot: false,
            },
            {
                client_id: "b",
                node_id: null,
                node_name: "财务复核",
                assignee_user_id: "",
                assignee_name: "",
                node_purpose: null,
                unsaved_purpose_slot: false,
            },
        ]
        const onChange = renderEditor(nodes)
        fireEvent.click(screen.getByText("增加节点"))
        expect(onChange).toHaveBeenCalledWith(
            expect.arrayContaining([
                expect.objectContaining({ node_name: "仓储复核" }),
            ]),
        )
        fireEvent.click(screen.getAllByText("下移")[0]!)
        expect(onChange).toHaveBeenLastCalledWith([nodes[1], nodes[0]])
        fireEvent.click(screen.getAllByText("删除")[1]!)
        expect(onChange).toHaveBeenLastCalledWith([nodes[0]])
        fireEvent.click(screen.getByText("选择审批人"))
        expect(onChange).toHaveBeenLastCalledWith([
            nodes[0],
            expect.objectContaining({
                assignee_user_id: "user-picked",
                assignee_name: "赵六",
            }),
        ])
    })

    it("does not allow deleting the sales order procurement node", () => {
        const nodes = seedDraftNodes(
            "sales_order",
            salesOrderSavedDraft().nodes,
        )
        renderEditor(nodes, vi.fn(), "sales_order")
        const firstNode = screen.getByTestId("approval-node-0")
        expect(firstNode.getAttribute("data-locked")).toBe("true")
        expect(nodes[0]?.node_purpose).toBe(SALES_ORDER_PROCUREMENT_PURPOSE)
        expect(screen.getAllByText("采购确认").length).toBeGreaterThan(0)
    })
})
