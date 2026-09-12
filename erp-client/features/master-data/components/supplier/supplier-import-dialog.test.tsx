import { afterEach, describe, expect, it, vi } from "vitest"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { SupplierImportDialog } from "./supplier-import-dialog"

const mocks = vi.hoisted(() => ({
    post: vi.fn(),
    read: vi.fn(),
}))
vi.mock("@/lib/api", () => ({ apiPost: mocks.post }))
vi.mock("@/features/master-data/lib/supplier-import", () => ({
    readSupplierFile: mocks.read,
}))
afterEach(() => {
    cleanup()
    vi.clearAllMocks()
})

describe("supplier background import", () => {
    it("preserves payload and request identity after an unknown response, then notifies parent", async () => {
        const rows = [
            { row_number: 2, cells: ["", "示例供应商"], parse_errors: [] },
        ]
        mocks.read.mockResolvedValue(rows)
        mocks.post
            .mockRejectedValueOnce(new Error("连接中断"))
            .mockResolvedValueOnce({ id: "job-1", total_count: 1 })
        const close = vi.fn()
        const submitted = vi.fn()
        render(
            <QueryClientProvider
                client={
                    new QueryClient({
                        defaultOptions: { queries: { retry: false } },
                    })
                }
            >
                <SupplierImportDialog onClose={close} onSubmitted={submitted} />
            </QueryClientProvider>,
        )
        fireEvent.change(screen.getByLabelText("选择供应商 Excel 文件"), {
            target: { files: [new File(["fixture"], "供应商.xlsx")] },
        })
        await screen.findByText("示例供应商")
        fireEvent.click(screen.getByText("提交后台导入"))
        await screen.findByText("重试核对")
        expect(close).not.toHaveBeenCalled()
        expect(
            (screen.getByLabelText("选择供应商 Excel 文件") as HTMLInputElement)
                .disabled,
        ).toBe(true)
        fireEvent.click(screen.getByText("重试核对"))
        await waitFor(() =>
            expect(submitted).toHaveBeenCalledWith(
                { id: "job-1", total_count: 1 },
                "供应商.xlsx",
            ),
        )
        expect(mocks.post.mock.calls[0][0]).toBe(
            "/admin/supplier-profiles/import/jobs",
        )
        expect(mocks.post.mock.calls[0][1]).toEqual(mocks.post.mock.calls[1][1])
        expect(mocks.post.mock.calls[0][1]).toMatchObject({
            file_name: "供应商.xlsx",
            rows,
        })
        expect(mocks.post.mock.calls[0][1].request_id).toBeTruthy()
        expect(close).toHaveBeenCalledOnce()
    })
})
