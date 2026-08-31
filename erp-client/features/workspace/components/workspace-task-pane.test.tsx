import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import {
    WorkspaceTaskFooter,
    WorkspaceTaskPane,
    useWorkspaceTaskPane,
} from "@/components/business/workspace-task-pane"

afterEach(cleanup)

function PaneContextProbe() {
    return <span>{useWorkspaceTaskPane() ? "作业面内" : "独立页面"}</span>
}

test("子组件可以区分作业面与独立页面", () => {
    const view = render(<PaneContextProbe />)
    expect(screen.getByText("独立页面")).toBeTruthy()

    view.rerender(
        <WorkspaceTaskPane header={<h2>任务标题</h2>}>
            <PaneContextProbe />
        </WorkspaceTaskPane>,
    )
    expect(screen.getByText("作业面内")).toBeTruthy()
})

test("标题栏和底栏固定，中间区域单独滚动", () => {
    render(
        <WorkspaceTaskPane
            header={<h2>任务标题</h2>}
            footer={<button type="button">通过</button>}
        >
            <p>任务正文</p>
        </WorkspaceTaskPane>,
    )

    const header = document.querySelector('[data-slot="workspace-task-header"]')
    const body = document.querySelector('[data-slot="workspace-task-body"]')
    const footer = document.querySelector('[data-slot="workspace-task-footer"]')

    expect(header).toBeTruthy()
    expect(body).toBeTruthy()
    expect(footer).toBeTruthy()
    expect(header?.contains(screen.getByText("任务标题"))).toBe(true)
    expect(body?.contains(screen.getByText("任务正文"))).toBe(true)
    expect(footer?.contains(screen.getByRole("button", { name: "通过" }))).toBe(
        true,
    )
    expect(body?.contains(header as Node)).toBe(false)
    expect(body?.contains(footer as Node)).toBe(false)
    expect(body?.className).toContain("overflow-auto")
})

test("子树里的操作按钮会送到作业面底栏", () => {
    render(
        <WorkspaceTaskPane header={<h2>任务标题</h2>}>
            <p>任务正文</p>
            <WorkspaceTaskFooter>
                <button type="button">提交</button>
            </WorkspaceTaskFooter>
        </WorkspaceTaskPane>,
    )

    const body = document.querySelector('[data-slot="workspace-task-body"]')
    const footer = document.querySelector('[data-slot="workspace-task-footer"]')
    const submit = screen.getByRole("button", { name: "提交" })

    expect(footer?.contains(submit)).toBe(true)
    expect(body?.contains(submit)).toBe(false)
})

test("不在作业面内时操作按钮仍在原位置", () => {
    render(
        <WorkspaceTaskFooter>
            <button type="button">提交</button>
        </WorkspaceTaskFooter>,
    )

    expect(screen.getByRole("button", { name: "提交" })).toBeTruthy()
    expect(
        document.querySelector('[data-slot="workspace-task-footer"]'),
    ).toBeNull()
})
