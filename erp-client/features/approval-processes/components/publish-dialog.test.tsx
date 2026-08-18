import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { afterEach, describe, expect, it, vi } from "vitest"

import { createApiError } from "@/lib/api/errors"

import { detailFixture } from "../fixtures"
import { REJECT_RESTART_COPY } from "../labels"
import { PublishDialog } from "./publish-dialog"

const mutateAsync = vi.fn()

vi.mock("../queries", () => ({
    usePublishDefinitionMutation: () => ({
        mutateAsync,
        isPending: false,
    }),
}))

const renderDialog = () => {
    const client = new QueryClient({
        defaultOptions: { mutations: { retry: false } },
    })
    const onConflict = vi.fn()
    const onOpenChange = vi.fn()
    render(
        <QueryClientProvider client={client}>
            <PublishDialog
                detail={detailFixture({
                    nodes: [
                        {
                            node_id: "n1",
                            node_key: "n1",
                            node_name: "仓储",
                            node_type: "USER_APPROVAL",
                            node_purpose: null,
                            display_order: 1,
                            assignee_user_id: "u1",
                            assignee_name_snapshot: "张三",
                        },
                        {
                            node_id: "n2",
                            node_key: "n2",
                            node_name: "财务",
                            node_type: "USER_APPROVAL",
                            node_purpose: null,
                            display_order: 2,
                            assignee_user_id: "u2",
                            assignee_name_snapshot: "李四",
                        },
                        {
                            node_id: "n3",
                            node_key: "n3",
                            node_name: "总复核",
                            node_type: "USER_APPROVAL",
                            node_purpose: null,
                            display_order: 3,
                            assignee_user_id: "u3",
                            assignee_name_snapshot: "王五",
                        },
                    ],
                })}
                lockVersion="3"
                open
                onOpenChange={onOpenChange}
                onConflict={onConflict}
                onPublished={vi.fn()}
            />
        </QueryClientProvider>,
    )
    return { onConflict, onOpenChange }
}

describe("publish dialog", () => {
    afterEach(() => {
        cleanup()
        mutateAsync.mockReset()
    })

    it("shows the linear path and fixed reject copy", () => {
        renderDialog()
        expect(screen.getByTestId("publish-path-preview").textContent).toBe(
            "张三 → 李四 → 王五",
        )
        expect(screen.getByTestId("publish-reject-copy").textContent).toBe(
            REJECT_RESTART_COPY,
        )
    })

    it("does not silently overwrite on stale version conflict", async () => {
        mutateAsync.mockRejectedValueOnce(
            createApiError({
                kind: "Http",
                status: 409,
                code: "APPROVAL_DEFINITION_VERSION_CONFLICT",
                message: "stale lock",
            }),
        )
        const { onConflict, onOpenChange } = renderDialog()
        fireEvent.click(screen.getByText("确认发布"))
        await waitFor(() => expect(onConflict).toHaveBeenCalled())
        expect(onOpenChange).not.toHaveBeenCalledWith(false)
        expect(screen.getByText("确认发布")).toBeTruthy()
        expect(
            screen.getByText("审批流程已被更新，请核对当前版本后重新确认。"),
        ).toBeTruthy()
        expect(mutateAsync).toHaveBeenCalledWith({
            definitionId: "def-stock-1",
            request: {
                expected_definition_lock_version: "3",
                idempotency_key: expect.stringMatching(/^publish:/),
            },
        })
    })
})
