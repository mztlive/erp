"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { OrganizationChangeDialog } from "@/features/organization/components/organization-change-dialog"
import { OrganizationEmptyByReason } from "@/features/organization/components/empty-by-reason"
import { OrganizationTree } from "@/features/organization/components/organization-tree"
import { OrganizationUnitPanel } from "@/features/organization/components/organization-unit-panel"
import {
    useOrganizationStateQuery,
    usePreviewOrganizationChangeMutation,
    useSubmitOrganizationChangeMutation,
} from "@/features/organization/hooks/queries"
import {
    EMPTY_CHANGE_DRAFT,
    type OrganizationChangeDraft,
} from "@/features/organization/lib/change-payload"
import { organizationEmptyReason } from "@/features/organization/lib/empty-reason"
import {
    KIND_LABEL,
    ORGANIZATION_BOUNDARY_NOTICE,
    PAGE_NARROW_CLASS,
} from "@/features/organization/lib/labels"
import {
    buildOrganizationForest,
    flattenTree,
    matchesOrganizationFilters,
} from "@/features/organization/lib/tree"
import {
    mergeOrganizationSearchParams,
    parseOrganizationSearchParams,
} from "@/features/organization/lib/url-state"
import { hasPermission } from "@/lib/permissions"

export function OrganizationPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const url = React.useMemo(
        () => parseOrganizationSearchParams(searchParams),
        [searchParams],
    )
    const pushUrl = React.useCallback(
        (patch: Partial<typeof url>) => {
            const next = { ...url, ...patch }
            router.replace(
                `${pathname}${mergeOrganizationSearchParams(searchParams, next)}`,
                { scroll: false },
            )
        },
        [pathname, router, searchParams, url],
    )

    const profileQuery = useAccountProfileQuery()
    const canManage = hasPermission(
        profileQuery.data?.permissions,
        "org_unit:manage",
    )
    const stateQuery = useOrganizationStateQuery()
    const previewMutation = usePreviewOrganizationChangeMutation()
    const submitMutation = useSubmitOrganizationChangeMutation()
    const [searchDraft, setSearchDraft] = React.useState(url.q ?? "")
    const [changeOpen, setChangeOpen] = React.useState(false)
    const [changeDraft, setChangeDraft] =
        React.useState<OrganizationChangeDraft>(EMPTY_CHANGE_DRAFT)

    React.useEffect(() => {
        setSearchDraft(url.q ?? "")
    }, [url.q])

    const view = stateQuery.data
    const forest = view ? buildOrganizationForest(view, url) : []
    const selected =
        flattenTree(forest).find((node) => node.unit.id === url.unitId) ??
        forest[0]
    const emptyReason = organizationEmptyReason({
        error: stateQuery.error,
        noScope: view?.emptyReason === "no_scope",
        filtered: Boolean(url.q || url.kind !== "all" || url.status !== "all"),
        empty: forest.length === 0,
        permissionPending: profileQuery.isPending,
    })

    const openChange = (
        operation: OrganizationChangeDraft["operation"],
        extras: Partial<OrganizationChangeDraft> = {},
    ) => {
        setChangeDraft({
            ...EMPTY_CHANGE_DRAFT,
            operation,
            orgUnitId: selected?.unit.id ?? "",
            parentId:
                operation === "create_unit" ? (selected?.unit.id ?? "") : "",
            ...extras,
        })
        setChangeOpen(true)
    }

    if (profileQuery.isPending || stateQuery.isPending) {
        return (
            <PageScaffold
                density="compact"
                className={`${styles.page} min-h-0 overflow-hidden`}
            >
                <div className="h-9 w-40 shrink-0 animate-pulse rounded-lg bg-muted" />
                <div className="min-h-0 flex-1 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold
            density="compact"
            className={`${styles.page} ${PAGE_NARROW_CLASS} min-h-0 overflow-hidden`}
        >
            <ListWorkspaceHeader
                className="shrink-0"
                eyebrow="系统"
                title="组织架构"
                description="维护部门与团队，管理成员归属和组织管理范围。"
            >
                {canManage ? (
                    <Button
                        id="organization-create-root"
                        type="button"
                        size="sm"
                        onClick={() =>
                            openChange("create_unit", {
                                parentId: "",
                                orgUnitId: "",
                            })
                        }
                    >
                        <PlusIcon className="size-3.5" aria-hidden="true" />
                        新建组织
                    </Button>
                ) : null}
            </ListWorkspaceHeader>

            <ListWorkSurface
                ariaLabel="组织树"
                className="min-h-0 overflow-hidden"
                tableClassName="flex flex-col overflow-hidden p-0"
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix="organization"
                        formAriaLabel="组织查询"
                        onSubmit={() =>
                            pushUrl({ q: searchDraft.trim() || undefined })
                        }
                        search={
                            <ListSearchField
                                id="organization-search"
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="按组织名称筛选"
                                aria-label="搜索组织"
                            />
                        }
                        primaryFilters={
                            <div className="flex min-w-0 flex-wrap gap-3">
                                <OptionCombobox
                                    id="organization-filter-kind"
                                    aria-label="类型"
                                    value={url.kind}
                                    options={[
                                        { value: "all", label: "全部类型" },
                                        ...Object.entries(KIND_LABEL).map(
                                            ([value, label]) => ({
                                                value,
                                                label,
                                            }),
                                        ),
                                    ]}
                                    onValueChange={(value) =>
                                        pushUrl({
                                            kind: (value ??
                                                "all") as typeof url.kind,
                                        })
                                    }
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    id="organization-filter-status"
                                    aria-label="状态"
                                    value={url.status}
                                    options={[
                                        { value: "all", label: "全部状态" },
                                        { value: "enabled", label: "启用" },
                                        { value: "disabled", label: "停用" },
                                    ]}
                                    onValueChange={(value) =>
                                        pushUrl({
                                            status: (value ??
                                                "all") as typeof url.status,
                                        })
                                    }
                                    allowClear={false}
                                />
                            </div>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: stateQuery.isFetching,
                            failed: stateQuery.isError,
                            resultCount: view
                                ? view.units.filter((unit) =>
                                      matchesOrganizationFilters(unit, url),
                                  ).length
                                : undefined,
                            noun: "个组织",
                        })}
                        chips={[
                            url.q
                                ? { key: "q", label: `名称：${url.q}` }
                                : null,
                            url.kind !== "all"
                                ? {
                                      key: "kind",
                                      label: KIND_LABEL[url.kind],
                                  }
                                : null,
                            url.status !== "all"
                                ? {
                                      key: "status",
                                      label:
                                          url.status === "enabled"
                                              ? "启用"
                                              : "停用",
                                  }
                                : null,
                        ].filter(
                            (chip): chip is { key: string; label: string } =>
                                chip != null,
                        )}
                        onClearChip={(key) => {
                            if (key === "q") pushUrl({ q: undefined })
                            if (key === "kind") pushUrl({ kind: "all" })
                            if (key === "status") pushUrl({ status: "all" })
                        }}
                        onClearAll={() =>
                            pushUrl({
                                q: undefined,
                                kind: "all",
                                status: "all",
                            })
                        }
                        hasPendingChanges={
                            (searchDraft.trim() || undefined) !== url.q
                        }
                    />
                }
                table={
                    emptyReason === "NO_MODULE_PERMISSION" ? (
                        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                            <OrganizationEmptyByReason
                                idPrefix="organization"
                                reason={emptyReason}
                            />
                        </div>
                    ) : stateQuery.isError && !view ? (
                        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                            <BusinessFailureState
                                id="organization-retry"
                                title="组织列表加载失败"
                                error={stateQuery.error}
                                onRetry={() => {
                                    void stateQuery.refetch()
                                }}
                            />
                        </div>
                    ) : emptyReason ? (
                        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                            <OrganizationEmptyByReason
                                idPrefix="organization"
                                reason={emptyReason}
                                onClearFilters={() =>
                                    pushUrl({
                                        q: undefined,
                                        kind: "all",
                                        status: "all",
                                    })
                                }
                            />
                        </div>
                    ) : (
                        <div className="grid min-h-0 min-w-0 flex-1 overflow-hidden grid-rows-[minmax(0,40%)_minmax(0,1fr)] lg:grid-cols-[288px_minmax(0,1fr)] lg:grid-rows-1">
                            <aside className="flex min-h-0 min-w-0 flex-col overflow-hidden border-b border-border bg-muted/20 lg:border-r lg:border-b-0">
                                <div className="flex shrink-0 items-center justify-between gap-3 px-4 pt-4 pb-4 lg:px-5 lg:pt-5">
                                    <h2 className="text-sm font-medium">
                                        组织目录
                                    </h2>
                                    <span className="text-xs text-muted-foreground">
                                        部门 / 团队
                                    </span>
                                </div>
                                <OrganizationTree
                                    nodes={forest}
                                    selectedId={selected?.unit.id}
                                    onSelect={(unitId) => pushUrl({ unitId })}
                                />
                            </aside>
                            <div className="min-h-0 min-w-0 overflow-x-hidden overflow-y-auto overscroll-contain">
                                {selected && view ? (
                                    <OrganizationUnitPanel
                                        node={selected}
                                        view={view}
                                        canManage={canManage}
                                        onChange={(operation, extras) =>
                                            openChange(
                                                operation as OrganizationChangeDraft["operation"],
                                                extras,
                                            )
                                        }
                                    />
                                ) : null}
                            </div>
                        </div>
                    )
                }
            />

            <div className="shrink-0 space-y-1 text-xs leading-5 text-muted-foreground">
                <p>{ORGANIZATION_BOUNDARY_NOTICE}</p>
                {view ? <p>当前范围：{view.scopeSummary}</p> : null}
            </div>

            {view && canManage ? (
                <OrganizationChangeDialog
                    open={changeOpen}
                    onOpenChange={setChangeOpen}
                    view={view}
                    draft={changeDraft}
                    expectedVersion={view.organizationVersion}
                    previewing={previewMutation.isPending}
                    submitting={submitMutation.isPending}
                    onPreview={(request) =>
                        previewMutation.mutateAsync(request)
                    }
                    onSubmit={async (request) => {
                        await submitMutation.mutateAsync(request)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
