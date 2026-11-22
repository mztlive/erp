"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowUpRightIcon, ChevronRightIcon } from "lucide-react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { CategoryDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { CategoryReviseDialog } from "@/features/master-data/components/category/category-form-dialogs"
import {
    ObjectCenterQueryState,
    ObjectCenterView,
} from "@/features/master-data/components/shared/object-center-view"
import { useMasterDataCenterQuery } from "@/features/master-data/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function CategoryObjectPage({
    stableId,
    section,
}: {
    stableId: string
    section?: string
}) {
    const router = useRouter()
    const query = useMasterDataCenterQuery("categories", stableId)
    const [reviseOpen, setReviseOpen] = React.useState(false)
    const [disableOpen, setDisableOpen] = React.useState(false)
    const returnTo = useSearchParams().get("returnTo")
    const hasValidReturnTo =
        returnTo === "/master-data/categories" ||
        Boolean(returnTo?.startsWith("/master-data/categories?"))
    const listHref =
        hasValidReturnTo && returnTo ? returnTo : "/master-data/categories"

    if (query.isPending || query.isError || !query.data) {
        return (
            <ObjectCenterQueryState
                idPrefix="master-data-category-object-view"
                title="商品分类详情"
                listHref={listHref}
                backVisibility={hasValidReturnTo ? "always" : undefined}
                isPending={query.isPending}
                isError={query.isError}
                error={query.error}
                onRetry={() => void query.refetch()}
                missing={!query.isPending && !query.isError && !query.data}
            />
        )
    }

    const data = query.data
    const hierarchy = data.categoryHierarchy
    const returnQuery =
        hasValidReturnTo && returnTo
            ? `?${new URLSearchParams({ returnTo })}`
            : ""
    const baseHref = `/master-data/categories/${data.stableId}${returnQuery}`

    const overviewExtra = (
        <div className="space-y-4 border-t border-border/60 pt-4">
            {hierarchy ? (
                <div className="space-y-4">
                    <div className="space-y-1.5">
                        <p className="text-xs font-medium text-muted-foreground">
                            层级路径
                        </p>
                        <nav
                            aria-label="分类层级路径"
                            className="flex flex-wrap items-center gap-1 text-body-compact"
                        >
                            <Link
                                id="master-data-category-object-path-root"
                                href={listHref}
                                className="text-muted-foreground transition-colors hover:text-primary"
                            >
                                全部分类
                            </Link>
                            {hierarchy.ancestors.map((ancestor) => (
                                <React.Fragment key={ancestor.stableId}>
                                    <ChevronRightIcon
                                        aria-hidden="true"
                                        className="size-3.5 text-muted-foreground/55"
                                    />
                                    <Link
                                        id={`master-data-category-object-ancestor-${toAutomationIdSegment(ancestor.stableId)}`}
                                        href={`/master-data/categories/${ancestor.stableId}${returnQuery}`}
                                        className="text-primary hover:underline"
                                    >
                                        {ancestor.name}
                                    </Link>
                                </React.Fragment>
                            ))}
                            <ChevronRightIcon
                                aria-hidden="true"
                                className="size-3.5 text-muted-foreground/55"
                            />
                            <span className="font-medium text-foreground">
                                {data.name}
                            </span>
                        </nav>
                    </div>

                    <div className="space-y-2">
                        <div className="flex flex-wrap items-baseline gap-2">
                            <p className="text-xs font-medium text-muted-foreground">
                                直接子分类
                            </p>
                            <span className="num text-xs text-muted-foreground">
                                {hierarchy.directChildren.length} 个
                                {hierarchy.descendantCount >
                                hierarchy.directChildren.length
                                    ? `（含全部下级 ${hierarchy.descendantCount} 个）`
                                    : ""}
                            </span>
                        </div>
                        {hierarchy.directChildren.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                                末级分类，暂无下级子分类。
                            </p>
                        ) : (
                            <ul className="divide-y divide-border/60 border-y border-border/60">
                                {hierarchy.directChildren.map((child) => (
                                    <li
                                        key={child.stableId}
                                        className="flex flex-wrap items-center justify-between gap-2 py-2"
                                    >
                                        <div className="flex min-w-0 flex-wrap items-center gap-2">
                                            <Link
                                                id={`master-data-category-object-child-${toAutomationIdSegment(child.stableId)}`}
                                                href={`/master-data/categories/${child.stableId}${returnQuery}`}
                                                className="font-medium text-foreground hover:text-primary"
                                            >
                                                {child.name}
                                            </Link>
                                            <span className="num text-xs text-muted-foreground">
                                                {child.code}
                                            </span>
                                        </div>
                                        <StatusBadge
                                            tone={child.lifecycleTone}
                                            appearance="quiet"
                                            label={child.lifecycleStatusLabel}
                                        />
                                    </li>
                                ))}
                            </ul>
                        )}
                    </div>
                </div>
            ) : null}

            <PermissionGuard permission="product:list">
                <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border/60 pt-4">
                    <div className="space-y-0.5">
                        <p className="text-body-compact font-medium text-foreground">
                            关联商品
                        </p>
                        <p className="text-xs text-muted-foreground">
                            前往商品列表查看归属该分类下的全部商品资料。
                        </p>
                    </div>
                    <Button
                        id="master-data-category-object-view-products"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        render={
                            <Link
                                href={`/master-data/products?productCategoryId=${encodeURIComponent(data.stableId)}`}
                            />
                        }
                    >
                        查看关联商品
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    </Button>
                </div>
            </PermissionGuard>
        </div>
    )

    return (
        <ObjectCenterView
            idPrefix="master-data-category-object-view"
            data={data}
            baseHref={baseHref}
            section={section}
            backVisibility={hasValidReturnTo ? "always" : undefined}
            overviewExtra={overviewExtra}
            onBack={() => router.push(listHref)}
            onRevise={() => setReviseOpen(true)}
            onDisable={() => setDisableOpen(true)}
            dialogs={
                <>
                    <PermissionGuard permission="product_category:update">
                        <CategoryReviseDialog
                            open={reviseOpen}
                            onOpenChange={setReviseOpen}
                            target={data}
                        />
                    </PermissionGuard>
                    <PermissionGuard permission="product_category:update">
                        <CategoryDisableDialog
                            open={disableOpen}
                            onOpenChange={setDisableOpen}
                            target={data}
                        />
                    </PermissionGuard>
                </>
            }
        />
    )
}
