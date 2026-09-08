import { cleanup, render, screen, within } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import type { DefinitionDetailView, DefinitionNodeView } from "../types"
import { DefinitionReadView } from "./definition-read-view"

afterEach(cleanup)

const node = (id: string, order: number, name: string): DefinitionNodeView => ({
    node_id: id,
    node_key: id,
    node_name: name,
    node_type: "APPROVAL",
    node_purpose: null,
    display_order: order,
    assignee_user_id: `${id}-user`,
    assignee_name_snapshot: `${name}负责人`,
})

const detail: DefinitionDetailView = {
    definition_id: "definition-1",
    document_type: "sales_order",
    document_type_label: "销售单",
    name: "销售单审批",
    definition_version: "1",
    status: "PUBLISHED",
    entry_node_key: "first",
    definition_lock_version: "1",
    nodes: [node("second", 2, "财务复核"), node("first", 1, "采购确认")],
    created_by: "admin",
    published_by: "admin",
    published_at: 1,
    retired_by: null,
    retired_at: null,
}

test("只读版本按审批顺序展示人员且不修改原始数据、不提供编辑入口", () => {
    render(<DefinitionReadView detail={detail} />)
    const rows = within(
        screen.getByRole("list", { name: "审批顺序" }),
    ).getAllByRole("listitem")
    expect(rows[0].textContent).toContain("采购确认")
    expect(rows[0].textContent).toContain("采购确认负责人")
    expect(rows[1].textContent).toContain("财务复核")
    expect(rows[1].textContent).toContain("完成流程")
    expect(detail.nodes[0].node_id).toBe("second")
    expect(screen.queryByRole("textbox")).toBeNull()
    expect(screen.queryByRole("button")).toBeNull()
})

test("销售单的空历史版本不生成草稿默认节点", () => {
    render(
        <DefinitionReadView
            detail={{ ...detail, status: "RETIRED", nodes: [] }}
        />,
    )
    expect(screen.getByText("此版本没有审批节点记录。")).toBeTruthy()
    expect(screen.queryByText("采购确认")).toBeNull()
    expect(screen.queryByRole("list", { name: "审批顺序" })).toBeNull()
})
