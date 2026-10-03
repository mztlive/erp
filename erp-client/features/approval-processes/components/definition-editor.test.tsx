import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type {
    DefinitionDetailView,
    ReplaceDefinitionNodesCommand,
} from "../types"
import { DefinitionEditor } from "./definition-editor"

const mutations = vi.hoisted(() => ({ replace: vi.fn() }))

vi.mock("../queries", () => ({
    useReplaceDefinitionNodesMutation: () => ({
        isPending: false,
        mutateAsync: mutations.replace,
    }),
    useEligibleAssigneesQuery: () => ({
        data: [{ user_id: "finance", name: "财务负责人" }],
        isFetching: false,
    }),
}))

vi.mock("./definition-flowchart", () => ({
    DefinitionFlowchart: () => null,
}))

const detail: DefinitionDetailView = {
    definition_id: "draft-1",
    document_type: "stock_adjustment",
    document_type_label: "库存调整",
    name: "库存调整审批",
    definition_version: 2,
    definition_lock_version: 3,
    status: "DRAFT",
    entry_node_key: "finance-review",
    nodes: [
        {
            node_id: "finance-review",
            node_key: "finance-review",
            node_name: "财务复核",
            node_type: "APPROVAL",
            node_purpose: null,
            display_order: 1,
            assignee_user_id: "finance",
            assignee_name_snapshot: "财务负责人",
        },
    ],
    created_by: "admin",
    published_by: null,
    published_at: null,
    retired_by: null,
    retired_at: null,
}

afterEach(() => {
    cleanup()
    mutations.replace.mockReset()
})

test("节点校验失败后补全名称恢复保存，并保留审批人及版本", async () => {
    mutations.replace.mockImplementation(
        async (command: ReplaceDefinitionNodesCommand) => ({
            ...detail,
            definition_lock_version: 4,
            nodes: [
                {
                    ...detail.nodes[0],
                    node_name: command.request.nodes[0].node_name,
                },
            ],
        }),
    )
    const onLockVersionChange = vi.fn()
    const { container } = render(
        <DefinitionEditor
            detail={detail}
            lockVersion="3"
            onLockVersionChange={onLockVersionChange}
        />,
    )
    const name = screen.getByRole("textbox", { name: "节点名称*" })
    const save = screen.getByRole("button", {
        name: "保存草稿",
    }) as HTMLButtonElement
    fireEvent.change(name, { target: { value: "" } })
    await waitFor(() => expect(save.disabled).toBe(true))
    fireEvent.submit(container.querySelector("form")!)
    await waitFor(() =>
        expect(
            screen.getAllByText(
                "请补全审批流程名称、每个节点的名称和审批人后再保存。",
            ).length,
        ).toBeGreaterThan(0),
    )
    expect(mutations.replace).not.toHaveBeenCalled()

    fireEvent.change(name, { target: { value: "财务重新复核" } })
    await waitFor(() => expect(save.disabled).toBe(false))
    fireEvent.click(save)
    await waitFor(() => expect(mutations.replace).toHaveBeenCalledTimes(1))
    expect(mutations.replace).toHaveBeenCalledWith(
        expect.objectContaining({
            definitionId: "draft-1",
            request: expect.objectContaining({
                expected_definition_lock_version: "3",
                nodes: [
                    expect.objectContaining({
                        node_id: "finance-review",
                        node_name: "财务重新复核",
                        assignee_user_id: "finance",
                        display_order: 1,
                    }),
                ],
            }),
        }),
    )
    await waitFor(() => expect(onLockVersionChange).toHaveBeenCalledWith("4"))
})
