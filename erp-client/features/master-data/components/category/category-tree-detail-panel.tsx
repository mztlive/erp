"use client"

import Link from "next/link"
import { BanIcon, HistoryIcon, PlusIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    FormalActionResult,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataListItem } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

/** 右侧分类详情：路径、版本与维护操作。 */
export function CategoryTreeDetailPanel({
    selected,
    selectedId,
    selectedPath,
    rows,
    onClearFilters,
    onOpenCreateChild,
    onReviseTarget,
    onDisableTarget,
}: {
    selected: MasterDataListItem | null
    selectedId: string | null
    selectedPath: string | undefined
    rows: readonly MasterDataListItem[]
    onClearFilters: () => void
    onOpenCreateChild: (item: MasterDataListItem) => void
    onReviseTarget: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    return (
        <section
            className={cn(surfacePanelClassName, "flex min-h-0 flex-col")}
            aria-label="分类详情"
        >
            <div className="border-b border-border/30 px-3 py-2">
                <h2 className="text-sm font-semibold">分类详情</h2>
            </div>
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
                {!selected && selectedId ? (
                    <div className="flex flex-col gap-2">
                        <p className="text-sm text-muted-foreground">
                            当前选中的分类不在筛选结果中。
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选后查看
                        </Button>
                    </div>
                ) : !selected ? (
                    <p className="text-sm text-muted-foreground">
                        在左侧选择一个分类，查看路径、版本并执行维护。
                    </p>
                ) : (
                    <>
                        <div className="space-y-1">
                            <div className="text-lg font-semibold">
                                {selected.name}
                            </div>
                            <div className="num text-sm text-muted-foreground">
                                {selected.stableNo} · v{selected.revisionNo}
                            </div>
                            <div className="text-xs text-muted-foreground">
                                路径：{selectedPath}
                            </div>
                        </div>
                        <div className="flex flex-wrap gap-2">
                            <BusinessStatusBadge
                                context="detail"
                                label={selected.lifecycleStatusLabel}
                                tone={selected.lifecycleTone}
                            />
                            <Badge
                                variant={
                                    selected.revisionTiming === "FUTURE"
                                        ? "warning"
                                        : "secondary"
                                }
                            >
                                {selected.revisionTimingLabel}
                            </Badge>
                        </div>
                        <dl className="grid gap-2 text-sm sm:grid-cols-2">
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    {masterDataCopy.categoryColCode}
                                </dt>
                                <dd className="num font-medium">
                                    {selected.dictionaryCode ??
                                        selected.keyFacts.find(
                                            (f) => f.label === "分类代码",
                                        )?.value ??
                                        "—"}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    {masterDataCopy.categoryColParent}
                                </dt>
                                <dd className="font-medium">
                                    {selected.parentStableId
                                        ? (rows.find(
                                              (r) =>
                                                  r.stableId ===
                                                  selected.parentStableId,
                                          )?.name ?? "—")
                                        : masterDataCopy.categoryParentRoot}
                                </dd>
                            </div>
                            <div className="sm:col-span-2">
                                <dt className="text-xs text-muted-foreground">
                                    {masterDataCopy.categoryColKind}
                                </dt>
                                <dd className="font-medium">
                                    {selected.productKind ??
                                        selected.keyFacts.find(
                                            (f) =>
                                                f.label === "适用商品类型",
                                        )?.value ??
                                        "—"}
                                </dd>
                            </div>
                        </dl>
                        <div className="flex flex-wrap gap-2 border-t border-border/30 pt-3">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/master-data/categories/${selected.stableId}?section=overview`}
                                    />
                                }
                            >
                                打开完整资料
                            </Button>
                            <span
                                title={
                                    !selected.allowedActions.includes(
                                        "CREATE_REVISION",
                                    )
                                        ? selected.actionBlockers.find(
                                              (b) =>
                                                  b.action ===
                                                  "CREATE_REVISION",
                                          )?.message
                                        : undefined
                                }
                                className="inline-flex"
                            >
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        !selected.allowedActions.includes(
                                            "CREATE_REVISION",
                                        )
                                    }
                                    onClick={() => onOpenCreateChild(selected)}
                                >
                                    <PlusIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                    {masterDataCopy.categoryAddChild}
                                </Button>
                            </span>
                            <span
                                title={
                                    !selected.allowedActions.includes(
                                        "CREATE_REVISION",
                                    )
                                        ? selected.actionBlockers.find(
                                              (b) =>
                                                  b.action ===
                                                  "CREATE_REVISION",
                                          )?.message
                                        : undefined
                                }
                                className="inline-flex"
                            >
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        !selected.allowedActions.includes(
                                            "CREATE_REVISION",
                                        )
                                    }
                                    onClick={() => onReviseTarget(selected)}
                                >
                                    <HistoryIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                    {masterDataCopy.actionUpdate}
                                </Button>
                            </span>
                            <span
                                title={
                                    !selected.allowedActions.includes("DISABLE")
                                        ? selected.actionBlockers.find(
                                              (b) => b.action === "DISABLE",
                                          )?.message
                                        : undefined
                                }
                                className="inline-flex"
                            >
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        !selected.allowedActions.includes(
                                            "DISABLE",
                                        )
                                    }
                                    onClick={() => onDisableTarget(selected)}
                                >
                                    <BanIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                    {masterDataCopy.actionDisable}
                                </Button>
                            </span>
                        </div>
                        {selected.primaryBlocker ? (
                            <FormalActionResult
                                status="blocked"
                                title="当前不可用"
                                description={selected.primaryBlocker}
                            />
                        ) : null}
                    </>
                )}
            </div>
        </section>
    )
}
