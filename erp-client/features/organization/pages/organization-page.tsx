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
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { EMPTY_CHANGE_DRAFT, type OrganizationChangeDraft } from "@/features/organization/lib/change-payload"
import { organizationEmptyReason } from "@/features/organization/lib/empty-reason"
import { KIND_LABEL, ORGANIZATION_BOUNDARY_NOTICE, PAGE_NARROW_CLASS } from "@/features/organization/lib/labels"
import { buildOrganizationForest, flattenTree } from "@/features/organization/lib/tree"
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
    const canList = hasPermission(profileQuery.data?.permissions, "org_unit:list")
    const canManage = hasPermission(
        profileQuery.data?.permissions,
        "org_unit:manage",
    )
    const stateQuery = useOrganizationStateQuery(canList)
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

    if (stateQuery.isPending && canList) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <div className="h-9 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-[32rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold
            density="compact"
            className={`${styles.page} ${PAGE_NARROW_CLASS}`}
        >
            <ListWorkspaceHeader
                eyebrow="系统"
                title="组织架构"
                description={ORGANIZATION_BOUNDARY_NOTICE}
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

            {view ? (
                <Alert>
                    <AlertTitle>当前范围</AlertTitle>
                    <AlertDescription>
                        {view.scopeSummary} · 组织版本 {view.organizationVersion}
                    </AlertDescription>
                </Alert>
            ) : null}

            <ListWorkSurface
                ariaLabel="组织树"
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
                            resultCount: view ? forest.length : undefined,
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
                    stateQuery.isError && !view ? (
                        <BusinessFailureState
                            id="organization-retry"
                            title="组织列表加载失败"
                            error={stateQuery.error}
                            onRetry={() => {
                                void stateQuery.refetch()
                            }}
                        />
                    ) : emptyReason ? (
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
                    ) : (
                        <div className="grid min-w-0 gap-6 overflow-x-hidden lg:grid-cols-[minmax(0,16rem)_minmax(0,1fr)]">
                            <OrganizationTree
                                nodes={forest}
                                selectedId={selected?.unit.id}
                                onSelect={(unitId) => pushUrl({ unitId })}
                            />
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
                    )
                }
            />

            {view && canManage ? (
                <OrganizationChangeDialog
                    open={changeOpen}
                    onOpenChange={setChangeOpen}
                    view={view}
                    draft={changeDraft}
                    expectedVersion={view.organizationVersion}
                    previewing={previewMutation.isPending}
                    submitting={submitMutation.isPending}
                    onPreview={(request) => previewMutation.mutateAsync(request)}
                    onSubmit={async (request) => {
                        await submitMutation.mutateAsync(request)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
