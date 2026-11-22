"use client"

import * as React from "react"
import { ArrowLeftIcon, ChevronRightIcon, SearchIcon } from "lucide-react"

import {
    BusinessFailureState,
    detailWorkspaceStyles,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useExpenseSourceQuery,
    useExpenseSourcesQuery,
} from "../hooks/expense-source-queries"
import {
    ExpensePurchaseCreateDialog,
    type ExpensePurchaseCreated,
} from "./expense-purchase-create-dialog"

/** 采购主入口：在采购页面选择来源，复用费用建单表单。关闭时卸载以清理来源和旧错误。 */
export function ExpensePurchaseStartDialog(props: {
    open: boolean
    onOpenChange: (open: boolean) => void
    salesOrderId?: string
    supplierId?: string
    onCreated: (created: ExpensePurchaseCreated) => void
}) {
    return props.open ? <ExpensePurchaseStartContent {...props} /> : null
}

function ExpensePurchaseStartContent({
    onOpenChange,
    salesOrderId,
    supplierId,
    onCreated,
}: {
    onOpenChange: (open: boolean) => void
    salesOrderId?: string
    supplierId?: string
    onCreated: (created: ExpensePurchaseCreated) => void
}) {
    const [selectedId, setSelectedId] = React.useState(salesOrderId)
    const [query, setQuery] = React.useState({
        q: "",
        page: 1,
        scopeVersion: undefined as string | undefined,
    })
    const list = useExpenseSourcesQuery(query, !selectedId)
    const detail = useExpenseSourceQuery(selectedId, Boolean(selectedId))
    const form = useAppForm({
        defaultValues: { q: "" },
        onSubmit: ({ value }) => {
            setQuery({ q: value.q.trim(), page: 1, scopeVersion: undefined })
        },
    })

    if (
        selectedId &&
        detail.data &&
        detail.isFetchedAfterMount &&
        !detail.isError
    ) {
        return (
            <ExpensePurchaseCreateDialog
                salesOrderId={selectedId}
                salesLines={detail.data.lines}
                sourceLabel={`${detail.data.order_no} · ${detail.data.customer_name ?? "客户未标注"}`}
                title="新建费用采购单"
                open
                onOpenChange={onOpenChange}
                onChangeSource={
                    salesOrderId ? undefined : () => setSelectedId(undefined)
                }
                prefill={
                    supplierId ? { supplierId, category: "other" } : undefined
                }
                onCreated={onCreated}
            />
        )
    }

    return (
        <Dialog open onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="procurement-expense-source-close"
                className="flex max-h-[min(90svh,44rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl"
            >
                <DialogHeader className="shrink-0 gap-2 border-b border-grid px-6 py-4 pr-14 text-left">
                    <DialogTitle>新建费用采购单 · 选择归属销售单</DialogTitle>
                    <DialogDescription>
                        先选择费用所属的销售单，再填写收款供应商和费用明细。仅显示协同范围内已生效的实物与服务销售单。
                    </DialogDescription>
                    {!selectedId ? (
                        <form
                            className="flex gap-2 pt-1"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <form.Field
                                name="q"
                                children={(field) => (
                                    <Input
                                        id="procurement-expense-source-search"
                                        aria-label="搜索归属销售单"
                                        placeholder="输入销售单号或客户名称搜索"
                                        value={field.state.value}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                    />
                                )}
                            />
                            <Button
                                id="procurement-expense-source-query"
                                type="submit"
                                size="sm"
                                className="shrink-0"
                            >
                                <SearchIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                                查询
                            </Button>
                        </form>
                    ) : null}
                </DialogHeader>

                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
                    {selectedId ? (
                        detail.isError ? (
                            <BusinessFailureState
                                id="procurement-expense-source-detail-retry"
                                title="来源销售单暂不可用"
                                error={detail.error}
                                onRetry={() => void detail.refetch()}
                            />
                        ) : (
                            <div
                                className="space-y-2.5"
                                role="status"
                                aria-label="正在核对来源销售单"
                            >
                                <div className="h-14 animate-pulse rounded-lg bg-muted/60" />
                                <div className="h-14 animate-pulse rounded-lg bg-muted/45" />
                            </div>
                        )
                    ) : list.isError ? (
                        <BusinessFailureState
                            id="procurement-expense-source-list-retry"
                            title="来源销售单加载失败"
                            error={list.error}
                            onRetry={() => {
                                setQuery({
                                    q: query.q,
                                    page: 1,
                                    scopeVersion: undefined,
                                })
                                if (query.page === 1) void list.refetch()
                            }}
                        />
                    ) : list.isPending || list.isFetching ? (
                        <div
                            className="space-y-2.5"
                            role="status"
                            aria-label="正在查询销售单"
                        >
                            <div className="h-14 animate-pulse rounded-lg bg-muted/60" />
                            <div className="h-14 animate-pulse rounded-lg bg-muted/50" />
                            <div className="h-14 animate-pulse rounded-lg bg-muted/40" />
                        </div>
                    ) : list.data.items.length === 0 ? (
                        <div className="rounded-lg bg-muted/35 px-4 py-8 text-center">
                            <p className="text-body-compact text-foreground">
                                {list.data.empty_reason === "no_scope"
                                    ? "尚未配置销售单协同范围"
                                    : "没有符合条件的销售单"}
                            </p>
                            <p className="mt-1 text-xs text-muted-foreground">
                                {list.data.empty_reason === "no_scope"
                                    ? "请联系管理员配置销售单协同范围后再创建费用采购单。"
                                    : "请尝试调整单号或客户名称关键词重新查询。"}
                            </p>
                        </div>
                    ) : (
                        <ul className="space-y-2">
                            {list.data.items.map((source) => (
                                <li
                                    key={source.id}
                                    className="flex items-center justify-between gap-4 rounded-lg border border-border/80 bg-card px-4 py-3 transition-colors hover:border-primary/45 hover:bg-muted/20"
                                >
                                    <div className="min-w-0 flex-1 space-y-0.5">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="num font-medium text-foreground">
                                                {source.order_no}
                                            </span>
                                            <Badge
                                                variant="secondary"
                                                className="font-normal"
                                            >
                                                {source.status_label}
                                            </Badge>
                                        </div>
                                        <p className="truncate text-xs text-muted-foreground">
                                            {source.customer_name ??
                                                "客户未标注"}
                                        </p>
                                    </div>
                                    <Button
                                        id={`procurement-expense-source-select-${toAutomationIdSegment(source.id)}`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => setSelectedId(source.id)}
                                    >
                                        选择
                                        <ChevronRightIcon
                                            aria-hidden="true"
                                            data-icon="inline-end"
                                        />
                                    </Button>
                                </li>
                            ))}
                        </ul>
                    )}
                </div>

                <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                    {selectedId && !salesOrderId ? (
                        <Button
                            id="procurement-expense-source-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={() => setSelectedId(undefined)}
                        >
                            <ArrowLeftIcon
                                aria-hidden="true"
                                data-icon="inline-start"
                            />
                            重新选择销售单
                        </Button>
                    ) : !selectedId && list.data ? (
                        <span className="num text-xs text-muted-foreground">
                            共 {list.data.total} 张 · 第 {query.page} 页
                        </span>
                    ) : (
                        <span />
                    )}

                    <div className="flex items-center justify-end gap-2">
                        {!selectedId && list.data ? (
                            <>
                                <Button
                                    id="procurement-expense-source-prev"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={
                                        query.page <= 1 || list.isFetching
                                    }
                                    onClick={() =>
                                        setQuery({
                                            ...query,
                                            page: query.page - 1,
                                            scopeVersion:
                                                list.data.scope_version,
                                        })
                                    }
                                >
                                    上一页
                                </Button>
                                <Button
                                    id="procurement-expense-source-next"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={
                                        query.page * list.data.page_size >=
                                            list.data.total || list.isFetching
                                    }
                                    onClick={() =>
                                        setQuery({
                                            ...query,
                                            page: query.page + 1,
                                            scopeVersion:
                                                list.data.scope_version,
                                        })
                                    }
                                >
                                    下一页
                                </Button>
                            </>
                        ) : null}
                        <Button
                            id="procurement-expense-source-cancel"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
