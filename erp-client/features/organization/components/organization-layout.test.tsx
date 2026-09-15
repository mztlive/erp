import { cleanup, render } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"

import { ListWorkspaceFilterBar } from "@/components/business/list-workspace"
import { PAGE_NARROW_CLASS } from "@/features/organization/lib/labels"
import { OrganizationChangeDialog } from "./organization-change-dialog"
import { DataScopeFormDialog } from "./data-scope-form-dialog"
import { OrganizationTree } from "./organization-tree"
import { EMPTY_CHANGE_DRAFT } from "@/features/organization/lib/change-payload"
import type { OrganizationStateView } from "@/features/organization/types"

const view: OrganizationStateView = {
    version: 1,
    organizationVersion: 1,
    policyVersion: 1,
    scopeVersion: "v",
    asOf: "2026-09-15T00:00:00Z",
    emptyReason: null,
    scopeSummary: "组织配置",
    ownershipBasis: "org_unit_configuration",
    people: [],
    roles: [],
    units: [
        {
            id: "sales",
            name: "销售部名称特别长需要换行避免横向滚动",
            parent_id: null,
            kind: "department",
            enabled: true,
            version: 1,
            reason: "初始化",
        },
    ],
    memberships: [],
    management: [],
}

afterEach(cleanup)

function wideMinWidthClasses(root: HTMLElement) {
    return [...root.querySelectorAll("[class]")].flatMap((node) => {
        const className = node.getAttribute("class") ?? ""
        return className.split(/\s+/).filter((token) => {
            const min = token.match(/^min-w-\[(\d+)px\]$/)
            const width = token.match(/^w-\[(\d+)px\]$/)
            const value = Number(min?.[1] ?? width?.[1] ?? 0)
            return value > 390
        })
    })
}

it("390px 容器内树、筛选条无强制宽于视口的最小宽度，并切断横向溢出", () => {
    const { container } = render(
        <div style={{ width: 390 }} className={PAGE_NARROW_CLASS}>
            <ListWorkspaceFilterBar
                idPrefix="organization"
                formAriaLabel="组织查询"
                onSubmit={() => undefined}
                search={<input aria-label="搜索组织" />}
                primaryFilters={
                    <div className="flex min-w-0 flex-wrap gap-3" />
                }
                resultStatus="共 1 个组织"
                chips={[{ key: "kind", label: "部门" }]}
                onClearChip={() => undefined}
                onClearAll={() => undefined}
            />
            <OrganizationTree
                nodes={[
                    {
                        unit: view.units[0]!,
                        children: [],
                        members: [],
                        management: [],
                    },
                ]}
                selectedId="sales"
                onSelect={() => undefined}
            />
        </div>,
    )
    const root = container.firstElementChild as HTMLElement
    expect(root.className).toContain("overflow-x-hidden")
    expect(root.className).toContain("min-w-0")
    expect(container.querySelector("nav")?.className).toContain(
        "overflow-x-hidden",
    )
    expect(wideMinWidthClasses(root)).toEqual([])
})

it("390px 对话框使用视口宽度并禁止横向滚动", () => {
    render(
        <div style={{ width: 390 }} className={PAGE_NARROW_CLASS}>
            <OrganizationChangeDialog
                open
                onOpenChange={() => undefined}
                view={view}
                draft={EMPTY_CHANGE_DRAFT}
                expectedVersion={1}
                previewing={false}
                submitting={false}
                onPreview={async (request) => ({
                    id: "r",
                    actor_id: "a",
                    request,
                    before: {
                        version: 1,
                        units: view.units,
                        memberships: [],
                        management: [],
                    },
                    after: {
                        version: 1,
                        units: view.units,
                        memberships: [],
                        management: [],
                    },
                    as_of: 1,
                })}
                onSubmit={async () => undefined}
            />
            <DataScopeFormDialog
                open
                onOpenChange={() => undefined}
                roles={[]}
                people={[]}
                units={[]}
                submitting={false}
                onSubmit={async () => undefined}
            />
        </div>,
    )
    const dialogs = [...document.querySelectorAll("[data-slot=dialog-content]")]
    expect(dialogs.length).toBeGreaterThan(0)
    expect(
        dialogs.every((node) =>
            (node.getAttribute("class") ?? "").includes("overflow-x-hidden"),
        ),
    ).toBe(true)
    expect(
        dialogs.every((node) =>
            (node.getAttribute("class") ?? "").includes(
                "w-[calc(100vw-1.5rem)]",
            ),
        ),
    ).toBe(true)
    expect(
        wideMinWidthClasses(document.body as unknown as HTMLElement),
    ).toEqual([])
})
