import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"
import {
    DOCUMENT_TYPES,
    NO_APPROVAL_DOCUMENT_TYPES,
    type DefinitionCatalogItem,
} from "../types"
import { DOCUMENT_TYPE_LABEL } from "../labels"
import { ProcessCatalog } from "./process-catalog"

afterEach(cleanup)

const item: DefinitionCatalogItem = {
    document_type: "purchase_order",
    document_type_label: "采购单",
    approval_requirement: "PROCESS_REQUIRED",
    published_version: "3",
    draft_version: "4",
    configuration_status: "PUBLISHED",
    allowed_actions: ["REPLACE_NODES"],
}

test("已发布和草稿同时展示，继续编辑传回对应单据", () => {
    const onContinueDraft = vi.fn()
    render(
        <ProcessCatalog
            items={[item]}
            permissions={["approval_process:edit"]}
            onCreateDraft={vi.fn()}
            onContinueDraft={onContinueDraft}
        />,
    )
    expect(screen.getByText("已发布")).toBeTruthy()
    expect(screen.getByText(/草稿.*尚未发布/)).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "继续编辑" }))
    expect(onContinueDraft).toHaveBeenCalledWith(item)
})

test("分组覆盖全部固定单据，无需审批没有写入口", () => {
    const items: DefinitionCatalogItem[] = DOCUMENT_TYPES.map((type) => ({
        ...item,
        document_type: type,
        document_type_label: DOCUMENT_TYPE_LABEL[type],
        approval_requirement: (
            NO_APPROVAL_DOCUMENT_TYPES as readonly string[]
        ).includes(type)
            ? "NO_APPROVAL"
            : "PROCESS_REQUIRED",
    }))
    render(
        <ProcessCatalog
            items={items}
            permissions={[]}
            onCreateDraft={vi.fn()}
            onContinueDraft={vi.fn()}
        />,
    )
    for (const type of DOCUMENT_TYPES)
        expect(screen.getByText(DOCUMENT_TYPE_LABEL[type])).toBeTruthy()
    expect(screen.queryByRole("button", { name: "继续编辑" })).toBeNull()
    expect(screen.getAllByText("无需审批")).toHaveLength(9)
    expect(screen.getAllByRole("link", { name: "查看流程" })).toHaveLength(11)
})
