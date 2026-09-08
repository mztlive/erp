import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type { EditorNode } from "../types"
import { DefinitionFlowchart } from "./definition-flowchart"

const nodes: EditorNode[] = [
    {
        client_id: "node-1",
        node_id: "node-1",
        node_name: "财务复核",
        assignee_user_id: "user-1",
        assignee_name: "张三",
        node_purpose: null,
        unsaved_purpose_slot: false,
    },
    {
        client_id: "node-2",
        node_id: null,
        node_name: "",
        assignee_user_id: "",
        assignee_name: "",
        node_purpose: null,
        unsaved_purpose_slot: false,
    },
]

afterEach(cleanup)

test("按顺序渲染开始节点链与结束", () => {
    render(<DefinitionFlowchart nodes={nodes} />)

    expect(screen.getByText("开始")).toBeTruthy()
    expect(screen.getByText("结束")).toBeTruthy()
    expect(screen.getByText("财务复核")).toBeTruthy()
    expect(screen.getByText("张三")).toBeTruthy()
    expect(screen.getByText("未命名节点")).toBeTruthy()
    expect(screen.getByText("待指定审批人")).toBeTruthy()
    expect(screen.getByText("共 2 个节点")).toBeTruthy()
})

test("空节点显示空状态", () => {
    render(<DefinitionFlowchart nodes={[]} />)

    expect(screen.getByText("暂无节点")).toBeTruthy()
    expect(
        screen.getByText("在左侧增加节点后，这里会生成流程图。"),
    ).toBeTruthy()
})

test("点击节点回传标识与序号", () => {
    const onSelect = vi.fn()
    render(<DefinitionFlowchart nodes={nodes} onSelect={onSelect} />)

    fireEvent.click(screen.getByRole("button", { name: /第 2 个节点/ }))

    expect(onSelect).toHaveBeenCalledWith("node-2", 1)
})

test("选中节点带 aria-current", () => {
    render(<DefinitionFlowchart nodes={nodes} selectedClientId="node-1" />)

    expect(
        screen
            .getByRole("button", { name: /第 1 个节点/ })
            .getAttribute("aria-current"),
    ).toBe("true")
})
