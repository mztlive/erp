"use client"

import * as React from "react"
import { useAppForm } from "@/components/form"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import {
    useExpenseSourceQuery,
    useExpenseSourcesQuery,
} from "../hooks/expense-source-queries"
import {
    ExpensePurchaseCreateDialog,
    type ExpensePurchaseCreated,
} from "./expense-purchase-create-dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"

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
                className="sm:max-w-2xl max-h-[85svh] overflow-y-auto"
            >
                <DialogHeader>
                    <DialogTitle>新建费用采购单 · 选择归属销售单</DialogTitle>
                    <DialogDescription>
                        选择这笔费用所属的销售单，再填写收款供应商和费用。仅显示协同范围内已生效的实物与服务销售单。
                    </DialogDescription>
                </DialogHeader>
                {selectedId ? (
                    detail.isError ? (
                        <BusinessFailureState
                            id="procurement-expense-source-detail-retry"
                            title="来源销售单暂不可用"
                            error={detail.error}
                            onRetry={() => void detail.refetch()}
                        />
                    ) : (
                        <p role="status">正在核对来源销售单…</p>
                    )
                ) : (
                    <>
                        <form
                            className="flex gap-2"
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
                                        placeholder="销售单号、客户名称"
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
                            >
                                查询
                            </Button>
                        </form>
                        {list.isError ? (
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
                            <p role="status">正在查询…</p>
                        ) : (
                            <>
                                <div className="divide-y">
                                    {list.data.items.map((source) => (
                                        <div
                                            key={source.id}
                                            className="flex items-center justify-between gap-4 py-3"
                                        >
                                            <div>
                                                <p className="font-medium">
                                                    {source.order_no}
                                                </p>
                                                <p className="text-sm text-muted-foreground">
                                                    {source.customer_name ??
                                                        "客户未标注"}{" "}
                                                    · {source.status_label}
                                                </p>
                                            </div>
                                            <Button
                                                id={`procurement-expense-source-select-${toAutomationIdSegment(source.id)}`}
                                                variant="outline"
                                                onClick={() =>
                                                    setSelectedId(source.id)
                                                }
                                            >
                                                选择
                                            </Button>
                                        </div>
                                    ))}
                                    {list.data.items.length === 0 ? (
                                        <p className="py-4 text-sm text-muted-foreground">
                                            {list.data.empty_reason ===
                                            "no_scope"
                                                ? "尚未配置销售单协同范围，请联系管理员配置后再创建费用采购单。"
                                                : "没有符合条件的销售单，请调整搜索条件。"}
                                        </p>
                                    ) : null}
                                </div>
                                <div className="flex justify-end items-center gap-3">
                                    <span className="text-sm text-muted-foreground">
                                        共 {list.data.total} 张 · 第{" "}
                                        {query.page} 页
                                    </span>
                                    <Button
                                        id="procurement-expense-source-prev"
                                        variant="outline"
                                        disabled={query.page <= 1}
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
                                        variant="outline"
                                        disabled={
                                            query.page * list.data.page_size >=
                                            list.data.total
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
                                </div>
                            </>
                        )}
                    </>
                )}
                {selectedId && !salesOrderId ? (
                    <Button
                        id="procurement-expense-source-back"
                        variant="outline"
                        onClick={() => setSelectedId(undefined)}
                    >
                        重新选择销售单
                    </Button>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
