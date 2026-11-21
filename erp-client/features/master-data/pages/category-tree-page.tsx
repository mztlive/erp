"use client"

import * as React from "react"
import {
    ChevronRightIcon,
    ChevronsDownUpIcon,
    ChevronsUpDownIcon,
    DownloadIcon,
    FolderTreeIcon,
    PlusIcon,
} from "lucide-react"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    BusinessFailureState,
    PageActions,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceQuietAction,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import {
    CategoryCreateDialog,
    CategoryReviseDialog,
} from "@/features/master-data/components/category/category-form-dialogs"
import { CategoryTreeDetailPanel } from "@/features/master-data/components/category/category-tree-detail-panel"
import { CategoryTreeList } from "@/features/master-data/components/category/category-tree-list"
import { CategoryTreeToolbar } from "@/features/master-data/components/category/category-tree-toolbar"
import { CategoryDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useMasterDataCategoryTree } from "@/features/master-data/hooks/use-master-data-category-tree"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

function TreeNavSkeleton() {
    return (
        <div
            role="status"
            className="space-y-2 px-3 py-3"
            aria-label="正在加载分类"
        >
            <span className="sr-only">正在加载分类…</span>
            <Skeleton className="h-7 w-full rounded-md" />
            <Skeleton className="ml-4 h-7 w-4/5 rounded-md" />
            <Skeleton className="h-7 w-11/12 rounded-md" />
            <Skeleton className="ml-4 h-7 w-3/4 rounded-md" />
            <Skeleton className="h-7 w-5/6 rounded-md" />
        </div>
    )
}

function WorkspaceSkeleton() {
    return (
        <div
            role="status"
            className="space-y-4 px-6 py-5"
            aria-label="正在加载分类资料"
        >
            <span className="sr-only">正在加载分类资料…</span>
            <div className="flex items-center justify-between border-b border-border/60 pb-4">
                <Skeleton className="h-5 w-36 rounded-md" />
                <Skeleton className="h-5 w-24 rounded-md" />
            </div>
            <div className="space-y-2.5 pt-1">
                <Skeleton className="h-11 w-full rounded-md" />
                <Skeleton className="h-11 w-full rounded-md" />
                <Skeleton className="h-11 w-full rounded-md" />
                <Skeleton className="h-11 w-full rounded-md" />
            </div>
        </div>
    )
}

/** 商品分类：飞书纯白通栏双栏工作面，页头内联状态视图，左树导航 + 右侧高密度分类管理表。 */
export function CategoryTreePage() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const mobileSearchRef = React.useRef<HTMLInputElement | null>(null)
    const state = useMasterDataCategoryTree(searchInputRef)
    const account = useAccountProfileQuery()
    const canExport =
        account.isSuccess &&
        hasPermission(account.data.permissions, "product_category:list")
    const canUpdate =
        account.isSuccess &&
        hasPermission(account.data.permissions, "product_category:update")
    const workspaceRef = React.useRef<HTMLDivElement | null>(null)
    const workspaceScrollKey = `category-workspace-scroll:${state.selectedId ?? "all"}:${state.q}:${state.lifecycleStatus}`

    React.useEffect(() => {
        const node = workspaceRef.current
        if (!node) return
        try {
            node.scrollTo({
                top: Number(sessionStorage.getItem(workspaceScrollKey) ?? 0),
            })
        } catch {
            /* 本地偏好不可用时从顶部浏览。 */
        }
    }, [workspaceScrollKey])

    const [navigationOpen, setNavigationOpen] = React.useState(false)
    const failed = state.fullQuery.isError || state.listQuery.isError
    const pending = state.fullQuery.isPending || state.listQuery.isPending
    const select = (id: string | null) => {
        state.setSelectedId(id)
        setNavigationOpen(false)
    }

    const enabledCount = React.useMemo(
        () =>
            state.rows.filter((row) => row.lifecycleStatus === "ENABLED")
                .length,
        [state.rows],
    )
    const disabledCount = React.useMemo(
        () =>
            state.rows.filter((row) => row.lifecycleStatus === "DISABLED")
                .length,
        [state.rows],
    )

    const nodes = state.selectedNode
        ? state.selectedNode.children
        : state.filterActive
          ? state.flat.filter((node) =>
                state.matchedIds.has(node.item.stableId),
            )
          : state.fullForest

    const navigation = (mobile: boolean) => {
        const prefix = mobile ? "category-mobile-nav" : "category-nav"
        const allSelected = !state.selectedId
        return (
            <>
                <div className="shrink-0 px-3 pt-3.5">
                    <CategoryTreeToolbar
                        idPrefix={`${prefix}-toolbar`}
                        searchInputRef={
                            mobile ? mobileSearchRef : searchInputRef
                        }
                        searchDraft={state.searchDraft}
                        setSearchDraft={state.setSearchDraft}
                        applyTreeFilters={state.applyTreeFilters}
                        clearFilters={state.clearFilters}
                        clearSearch={state.clearSearch}
                        filterActive={state.filterActive}
                        hasPendingChanges={state.hasPendingChanges}
                        loading={state.listQuery.isFetching}
                        matchedCount={state.matchedRows.length}
                    />
                    <Button
                        id={`${prefix}-all`}
                        type="button"
                        variant="ghost"
                        className={cn(
                            "h-8 w-full justify-start gap-2 rounded-md px-2.5 text-body-compact shadow-none",
                            allSelected
                                ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
                                : "font-normal text-foreground/85 hover:bg-muted/80 hover:text-foreground",
                        )}
                        aria-pressed={allSelected}
                        onClick={() => select(null)}
                    >
                        <FolderTreeIcon
                            aria-hidden="true"
                            className={cn(
                                "size-3.5 shrink-0",
                                allSelected
                                    ? "text-primary"
                                    : "text-muted-foreground/80",
                            )}
                        />
                        <span className="truncate">
                            {state.filterActive ? "全部匹配分类" : "全部分类"}
                        </span>
                        <span
                            className={cn(
                                "num ml-auto text-xs",
                                allSelected
                                    ? "font-medium text-sidebar-accent-foreground/80"
                                    : "text-muted-foreground/75",
                            )}
                        >
                            {state.filterActive
                                ? state.matchedRows.length
                                : state.rows.length}
                        </span>
                    </Button>
                </div>

                <div className="mx-3 mt-3 mb-1.5 flex shrink-0 items-center justify-between border-t border-border/60 pt-2.5">
                    <span className="text-xs font-medium text-muted-foreground/80">
                        分类目录
                    </span>
                    <div className="flex items-center gap-0.5">
                        <Button
                            id={`${prefix}-expand`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            className="h-6 gap-1 px-1.5 text-xs font-normal text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                            onClick={state.expandAll}
                        >
                            <ChevronsUpDownIcon
                                aria-hidden="true"
                                className="size-3"
                            />
                            展开
                        </Button>
                        <Button
                            id={`${prefix}-collapse`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            className="h-6 gap-1 px-1.5 text-xs font-normal text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                            onClick={state.collapseAll}
                        >
                            <ChevronsDownUpIcon
                                aria-hidden="true"
                                className="size-3"
                            />
                            收起
                        </Button>
                    </div>
                </div>

                {pending ? (
                    <TreeNavSkeleton />
                ) : failed ? (
                    <p className="px-4 py-8 text-center text-xs text-muted-foreground">
                        分类加载失败，请重试。
                    </p>
                ) : (
                    <CategoryTreeList
                        idPrefix={`${prefix}-tree`}
                        forest={state.forest}
                        expanded={state.expanded}
                        selectedId={state.selectedId}
                        onToggle={state.toggle}
                        onSelect={(item) => select(item.stableId)}
                        matchedIds={state.matchedIds}
                        filterActive={state.filterActive}
                        query={state.q.trim()}
                        onClearFilters={state.clearFilters}
                        onOpenCreateRoot={() => {
                            setNavigationOpen(false)
                            state.openCreateRoot()
                        }}
                        canCreate={state.canCreate}
                    />
                )}
            </>
        )
    }

    return (
        <PageScaffold
            density="compact"
            className={`${styles.page} lg:min-h-0 lg:overflow-hidden`}
        >
            <ListWorkspaceHeader
                className="shrink-0"
                eyebrow=""
                title="商品分类"
                views={
                    <ListWorkspaceViews
                        ariaLabel="商品分类启用状态"
                        variant="inline"
                        items={[
                            {
                                id: "master-data-category-tree-view-all",
                                label: "全部分类",
                                count: pending ? undefined : state.rows.length,
                                active: state.lifecycleStatus === "all",
                                onClick: () => state.setLifecycleStatus("all"),
                            },
                            {
                                id: "master-data-category-tree-view-enabled",
                                label: "当前启用",
                                count: pending ? undefined : enabledCount,
                                active: state.lifecycleStatus === "enabled",
                                onClick: () =>
                                    state.setLifecycleStatus("enabled"),
                            },
                            {
                                id: "master-data-category-tree-view-disabled",
                                label: "当前停用",
                                count: pending ? undefined : disabledCount,
                                active: state.lifecycleStatus === "disabled",
                                onClick: () =>
                                    state.setLifecycleStatus("disabled"),
                            },
                        ]}
                    />
                }
            >
                <PageActions
                    size="sm"
                    actions={[
                        listWorkspaceQuietAction({
                            id: "master-data-category-tree-export",
                            actionKey: "export",
                            hidden: !canExport,
                            label: "导出",
                            icon: DownloadIcon,
                            disabled:
                                failed ||
                                state.listQuery.isFetching ||
                                !state.matchedRows.length,
                            onClick: state.onExport,
                        }),
                        {
                            id: "master-data-category-tree-create-root",
                            actionKey: "create-root",
                            label: "新建分类",
                            icon: PlusIcon,
                            hidden: !state.canCreate,
                            disabled: !state.canCreate,
                            title: !state.canCreate
                                ? state.createBlockedReason
                                : undefined,
                            onClick: state.openCreateRoot,
                        },
                    ]}
                />
            </ListWorkspaceHeader>

            <div
                data-slot="list-work-surface"
                className="flex min-h-[32rem] min-w-0 flex-1 flex-col bg-card lg:min-h-0 lg:overflow-hidden"
            >
                <div className="border-b border-border/60 bg-surface-sunken/50 px-4 py-2.5 lg:hidden">
                    <Button
                        id="category-mobile-nav-open"
                        type="button"
                        variant="outline"
                        size="sm"
                        className="w-full justify-between bg-card shadow-none"
                        onClick={() => setNavigationOpen(true)}
                    >
                        <span className="flex min-w-0 items-center gap-2">
                            <FolderTreeIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0 text-primary"
                            />
                            <span className="truncate font-medium">
                                {state.selected?.name ?? "全部分类"}
                            </span>
                        </span>
                        <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                            切换目录{state.filterActive ? " · 已筛选" : ""}
                            <ChevronRightIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                        </span>
                    </Button>
                </div>

                <div className="grid min-h-0 min-w-0 flex-1 lg:grid-cols-[272px_minmax(0,1fr)] lg:overflow-hidden xl:grid-cols-[288px_minmax(0,1fr)]">
                    <aside
                        aria-label="分类导航"
                        className="hidden min-h-0 flex-col overflow-hidden border-r border-border/60 bg-surface-sunken/45 lg:flex"
                    >
                        {navigation(false)}
                    </aside>
                    <div
                        id="category-workspace-scroll"
                        ref={workspaceRef}
                        className="min-w-0 bg-card lg:min-h-0 lg:overflow-y-auto lg:overscroll-contain"
                        onScroll={(event) => {
                            try {
                                sessionStorage.setItem(
                                    workspaceScrollKey,
                                    String(event.currentTarget.scrollTop),
                                )
                            } catch {
                                /* 不影响浏览。 */
                            }
                        }}
                        aria-busy={pending || state.listQuery.isPlaceholderData}
                    >
                        {pending ? (
                            <WorkspaceSkeleton />
                        ) : failed ? (
                            <div className="p-6">
                                <BusinessFailureState
                                    error={
                                        state.fullQuery.error ??
                                        state.listQuery.error
                                    }
                                    action={
                                        <Button
                                            id="category-workspace-retry"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => {
                                                void state.fullQuery.refetch()
                                                void state.listQuery.refetch()
                                            }}
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            </div>
                        ) : state.listQuery.isPlaceholderData ? (
                            <WorkspaceSkeleton />
                        ) : (
                            <CategoryTreeDetailPanel
                                selected={state.selected}
                                selectedId={state.selectedId}
                                selectedNode={state.selectedNode}
                                rows={state.rows}
                                rootCount={state.fullForest.length}
                                nodes={nodes}
                                filterActive={state.filterActive}
                                matchedIds={state.matchedIds}
                                query={state.q.trim()}
                                returnTo={state.returnTo}
                                canCreate={state.canCreate}
                                canUpdate={canUpdate}
                                onSelect={select}
                                onClearFilters={state.clearFilters}
                                onOpenCreateRoot={state.openCreateRoot}
                                onOpenCreateChild={state.openCreateChild}
                                onReviseTarget={state.setReviseTarget}
                                onMoveTarget={state.setMoveTarget}
                                onDisableTarget={state.setDisableTarget}
                            />
                        )}
                    </div>
                </div>
            </div>

            <Sheet open={navigationOpen} onOpenChange={setNavigationOpen}>
                <SheetContent
                    closeButtonId="category-mobile-nav-close"
                    side="left"
                    className="w-[min(340px,100vw)] bg-surface-sunken/45"
                >
                    <SheetHeader>
                        <SheetTitle>分类目录</SheetTitle>
                        <SheetDescription>
                            查找分类并查看资料或下级分类。
                        </SheetDescription>
                    </SheetHeader>
                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                        {navigation(true)}
                    </div>
                </SheetContent>
            </Sheet>
            <PermissionGuard permission="product_category:create">
                <CategoryCreateDialog
                    key={`cat-create-${state.createParentId ?? "root"}-${state.createOpen}`}
                    open={state.createOpen}
                    onOpenChange={state.setCreateOpen}
                    defaultParentId={state.createParentId}
                    onCreated={state.onCreated}
                />
            </PermissionGuard>
            <PermissionGuard permission="product_category:update">
                <CategoryReviseDialog
                    open={state.reviseTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setReviseTarget(null)
                    }}
                    target={state.reviseTarget}
                />
            </PermissionGuard>
            <PermissionGuard permission="product_category:update">
                <CategoryReviseDialog
                    mode="move"
                    open={state.moveTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setMoveTarget(null)
                    }}
                    target={state.moveTarget}
                />
            </PermissionGuard>
            <PermissionGuard permission="product_category:update">
                <CategoryDisableDialog
                    open={state.disableTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setDisableTarget(null)
                    }}
                    target={state.disableTarget}
                />
            </PermissionGuard>
        </PageScaffold>
    )
}
