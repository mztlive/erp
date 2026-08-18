import { render, screen } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { describe, expect, it, vi } from "vitest"

import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { detailFixture, salesOrderEmptyDraft } from "../fixtures"
import { DefinitionEditor } from "./definition-editor"

vi.mock("./node-list-editor", () => ({
    NodeListEditor: ({
        nodes,
        readOnly,
    }: {
        nodes: Array<{ node_name: string; unsaved_purpose_slot?: boolean }>
        readOnly: boolean
    }) => (
        <div
            data-testid="node-list"
            data-readonly={readOnly ? "true" : "false"}
        >
            {nodes.map((node) => (
                <div key={node.node_name}>{node.node_name}</div>
            ))}
        </div>
    ),
}))

vi.mock("../queries", () => ({
    useReplaceDefinitionNodesMutation: () => ({
        mutateAsync: vi.fn(),
        isPending: false,
    }),
}))

const renderEditor = (
    detail = detailFixture(),
    lockVersion = detail.definition_lock_version,
) => {
    const client = new QueryClient({
        defaultOptions: {
            mutations: { retry: false },
            queries: { retry: false },
        },
    })
    render(
        <QueryClientProvider client={client}>
            <DefinitionEditor
                detail={detail}
                lockVersion={lockVersion}
                onLockVersionChange={vi.fn()}
            />
        </QueryClientProvider>,
    )
}

describe("definition editor", () => {
    it("keeps published and retired versions read only", () => {
        renderEditor(detailFixture({ status: "PUBLISHED" }))
        expect(screen.getByText("此版本只读")).toBeTruthy()
        expect(screen.getByText("此版本不可修改")).toBeTruthy()
        expect(screen.queryByText("保存草稿")).toBeNull()
        expect(
            screen.getByTestId("node-list").getAttribute("data-readonly"),
        ).toBe("true")
    })

    it("shows the unsaved procurement slot for SalesOrder empty drafts", () => {
        renderEditor(salesOrderEmptyDraft())
        expect(screen.getByText("采购确认")).toBeTruthy()
        expect(screen.getByText("保存草稿")).toBeTruthy()
    })

    it("is a client component and does not fetch in a server module", () => {
        const here = dirname(fileURLToPath(import.meta.url))
        const source = readFileSync(join(here, "definition-editor.tsx"), "utf8")
        expect(source.startsWith('"use client"')).toBe(true)
        expect(source).not.toMatch(/cookies\(|headers\(|getServerSideProps/)
    })
})
