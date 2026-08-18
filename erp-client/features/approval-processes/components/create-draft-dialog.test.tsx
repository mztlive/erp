import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { afterEach, describe, expect, it, vi } from "vitest"

import { catalogFixture } from "../fixtures"
import { CreateDraftDialog } from "./create-draft-dialog"

const mutateAsync = vi.fn()

vi.mock("../queries", () => ({
    useCreateDefinitionDraftMutation: () => ({
        mutateAsync,
        isPending: false,
    }),
}))

const renderDialog = (publishedVersion: string | null = null) => {
    const item = catalogFixture({
        stock_adjustment: {
            published_version: publishedVersion,
            allowed_actions: ["CREATE_DRAFT"],
        },
    }).find((row) => row.document_type === "stock_adjustment")!
    const onCreated = vi.fn()
    const client = new QueryClient({
        defaultOptions: { mutations: { retry: false } },
    })
    render(
        <QueryClientProvider client={client}>
            <CreateDraftDialog
                item={item}
                open
                onOpenChange={vi.fn()}
                onCreated={onCreated}
            />
        </QueryClientProvider>,
    )
    return { onCreated, item }
}

describe("create draft dialog", () => {
    afterEach(() => {
        cleanup()
    })

    it("requires an explicit source and disables copy when unpublished", () => {
        renderDialog(null)
        const copy = screen.getByLabelText("复制当前已发布版本")
        expect((copy as HTMLInputElement).disabled).toBe(true)
        expect((copy as HTMLInputElement).checked).toBe(false)
    })

    it("submits EMPTY without a source definition id", async () => {
        mutateAsync.mockResolvedValue({
            definition_id: "def-new",
            document_type: "stock_adjustment",
        })
        renderDialog("2")
        fireEvent.change(screen.getByLabelText("审批流程名称"), {
            target: { value: "库存调整审批" },
        })
        fireEvent.click(screen.getByLabelText("空白流程"))
        fireEvent.click(screen.getByText("创建草稿"))
        await waitFor(() => expect(mutateAsync).toHaveBeenCalled())
        expect(mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                document_type: "stock_adjustment",
                draft_source: "EMPTY",
                name: "库存调整审批",
            }),
        )
        const payload = mutateAsync.mock.calls[0]?.[0] as Record<
            string,
            unknown
        >
        expect(payload).not.toHaveProperty("source_definition_id")
    })
})
