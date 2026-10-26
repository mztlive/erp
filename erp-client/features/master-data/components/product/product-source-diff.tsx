"use client"

import * as React from "react"
import { RefreshCwIcon } from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { BusinessFailureState } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useProductSourceDiffQuery,
    useResolveProductSourceDiffMutation,
} from "../../hooks/use-product-source-diff"
import { newIdempotencyKey } from "../../lib/product-editor-model"
import {
    PRODUCT_SOURCE_FIELD_LABELS,
    type ProductSourceDecision,
    type ProductSourceDifference,
} from "../../api/types-product-source"
import { SourceFieldEdit } from "./product-source-edit"
import { canAdoptSource, SourceValueComparison } from "./product-source-values"

function SourceDifference({
    difference,
    disabled,
    pending,
    onResolve,
}: {
    difference: ProductSourceDifference
    disabled: boolean
    pending: boolean
    onResolve: (decisions: readonly ProductSourceDecision[]) => Promise<boolean>
}) {
    const [editing, setEditing] = React.useState(false)
    const prefix = `product-source-field-${toAutomationIdSegment(difference.field)}${difference.sku_id ? `-${toAutomationIdSegment(difference.sku_id)}` : ""}`
    const adoptBlocked =
        difference.verification_required || !canAdoptSource(difference.supplier)
    return (
        <section
            className="min-w-0 space-y-4 rounded-lg border p-4 md:p-5"
            aria-labelledby={`${prefix}-title`}
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 id={`${prefix}-title`} className="font-medium">
                    {PRODUCT_SOURCE_FIELD_LABELS[difference.field]}
                    {difference.sku_no ? ` · ${difference.sku_no}` : ""}
                </h3>
                {difference.manually_maintained ? (
                    <Badge variant="outline">公司已人工编辑</Badge>
                ) : null}
            </div>
            <SourceValueComparison
                company={difference.company}
                supplier={difference.supplier}
                idPrefix={prefix}
            />
            {adoptBlocked ? (
                <p className="text-sm text-warning-soft-foreground">
                    {difference.application_blocker ??
                        "供应商图片或资料尚待核验，暂不能采用。可以保留公司版本，完成核验后再处理更新。"}
                </p>
            ) : null}
            {editing ? (
                <SourceFieldEdit
                    key={prefix}
                    prefix={prefix}
                    difference={difference}
                    disabled={disabled || pending}
                    onCancel={() => setEditing(false)}
                    onConfirm={async (decision) => {
                        if (await onResolve([decision])) setEditing(false)
                    }}
                />
            ) : (
                <div className="flex flex-wrap justify-end gap-2">
                    <Button
                        id={`${prefix}-edit`}
                        type="button"
                        variant="ghost"
                        disabled={disabled || pending}
                        onClick={() => setEditing(true)}
                    >
                        编辑后确认
                    </Button>
                    <Button
                        id={`${prefix}-keep`}
                        type="button"
                        variant="outline"
                        disabled={disabled || pending}
                        onClick={() =>
                            void onResolve([
                                {
                                    field: difference.field,
                                    ...(difference.sku_id
                                        ? { sku_id: difference.sku_id }
                                        : {}),
                                    action: "KEEP",
                                },
                            ])
                        }
                    >
                        保留公司版本
                    </Button>
                    <Button
                        id={`${prefix}-adopt`}
                        type="button"
                        disabled={disabled || pending || adoptBlocked}
                        onClick={() =>
                            void onResolve([
                                {
                                    field: difference.field,
                                    ...(difference.sku_id
                                        ? { sku_id: difference.sku_id }
                                        : {}),
                                    action: "ADOPT",
                                },
                            ])
                        }
                    >
                        采用供应商版本
                    </Button>
                </div>
            )}
        </section>
    )
}

export function ProductSourceDiff({
    productId,
    enabled,
    canRevise,
    dirty,
    busy,
    openRequested = false,
    onProcessingChange,
}: {
    productId: string
    enabled: boolean
    canRevise: boolean
    dirty: boolean
    busy: boolean
    openRequested?: boolean
    onProcessingChange?: (processing: boolean) => void
}) {
    const query = useProductSourceDiffQuery(productId, enabled)
    const mutation = useResolveProductSourceDiffMutation(productId)
    const [open, setOpen] = React.useState(openRequested)
    const [success, setSuccess] = React.useState<string | null>(null)
    const [needsRefresh, setNeedsRefresh] = React.useState(false)
    const requestRef = React.useRef<{ payload: string; key: string } | null>(
        null,
    )
    const view = query.data
    const disabled =
        !canRevise || dirty || busy || needsRefresh || query.isFetching
    const refresh = async () => {
        const result = await query.refetch()
        if (result.isSuccess) {
            setNeedsRefresh(false)
            mutation.reset()
            requestRef.current = null
        }
    }
    const resolve = async (decisions: readonly ProductSourceDecision[]) => {
        if (!view || disabled || mutation.isPending) return false
        const payload = JSON.stringify([
            view.product_version,
            view.source_version,
            decisions,
        ])
        if (requestRef.current?.payload !== payload)
            requestRef.current = {
                payload,
                key: newIdempotencyKey("source-diff"),
            }
        onProcessingChange?.(true)
        try {
            await mutation.mutateAsync({
                idempotency_key: requestRef.current.key,
                expected_product_version: view.product_version,
                expected_source_version: view.source_version,
                expected_sku_versions: view.sku_versions,
                decisions,
            })
            requestRef.current = null
            setSuccess(`已确认 ${decisions.length} 项资料更新`)
            return true
        } catch {
            // 写入失败后要求重新核对双方版本，保留当前比较内容供阅读。
            setNeedsRefresh(true)
            return false
        } finally {
            onProcessingChange?.(false)
        }
    }
    if (!enabled) return null
    if (query.isError && !open)
        return (
            <Alert variant="warning">
                <AlertTitle>供应商资料更新暂不可查</AlertTitle>
                <AlertDescription>
                    {getErrorMessage(query.error)}
                    <Button
                        id="product-source-diff-retry"
                        type="button"
                        variant="link"
                        onClick={() => void refresh()}
                    >
                        重新读取
                    </Button>
                </AlertDescription>
            </Alert>
        )
    if (!query.isError && !view) return null
    const fieldsLabel = [
        ...new Set(
            view?.fields.map(
                (field) => PRODUCT_SOURCE_FIELD_LABELS[field.field],
            ),
        ),
    ].join("、")
    const allAdoptable =
        view?.fields.every(
            (field) =>
                !field.verification_required && canAdoptSource(field.supplier),
        ) ?? false
    const observed = view
        ? new Date(view.observed_at * 1000).toLocaleString("zh-CN")
        : ""
    return (
        <>
            <Alert variant={view?.pending_count ? "warning" : "info"}>
                <AlertTitle>
                    {view?.pending_count
                        ? `资料更新待确认 · ${view.pending_count} 项`
                        : "商品资料已关联供应商同步"}
                </AlertTitle>
                <AlertDescription>
                    <p>
                        {view?.source.supplier_name || "供应商资料"}
                        {observed ? ` · 最近读取 ${observed}` : ""}
                    </p>
                    <p>
                        {view?.pending_count
                            ? `${fieldsLabel}有更新，当前仍保留公司内容。`
                            : "未人工修改的展示资料随来源更新；人工编辑的字段出现变化时提示确认。公司分类、销售价格和上下架由公司维护。"}
                    </p>
                    {view?.pending_count ? (
                        <Button
                            id="product-source-diff-open"
                            type="button"
                            variant="outline"
                            size="sm"
                            className="mt-2"
                            onClick={() => {
                                setSuccess(null)
                                setOpen(true)
                            }}
                        >
                            查看差异
                        </Button>
                    ) : null}
                </AlertDescription>
            </Alert>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (!mutation.isPending) setOpen(next)
                }}
            >
                <DialogContent
                    closeButtonId="product-source-diff-close"
                    className="flex max-h-[90dvh] flex-col sm:max-w-5xl"
                >
                    <DialogHeader>
                        <DialogTitle>确认供应商商品资料更新</DialogTitle>
                        <DialogDescription>
                            逐项核对公司内容与供应商最新资料。采用后恢复该字段自动同步；保留或编辑后确认，本次相同更新不再提醒。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="min-h-0 min-w-0 flex-1 space-y-4 overflow-y-auto pr-1">
                        {query.isError ? (
                            <BusinessFailureState
                                title="资料差异读取失败"
                                error={query.error}
                                onRetry={() => void refresh()}
                            />
                        ) : null}
                        {dirty ? (
                            <Alert variant="warning">
                                <AlertTitle>请先保存或取消本页修改</AlertTitle>
                                <AlertDescription>
                                    当前可以查看差异，处理更新前需先完成商品编辑。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {!canRevise ? (
                            <Alert>
                                <AlertDescription>
                                    当前账号仅可查看，确认更新需要商品维护权限。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {mutation.error ? (
                            <Alert variant="destructive">
                                <AlertTitle>本次确认未完成</AlertTitle>
                                <AlertDescription>
                                    {getErrorMessage(mutation.error)}
                                    。请重新读取并核对最新内容后再确认。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {success ? (
                            <Alert variant="success">
                                <AlertDescription>{success}</AlertDescription>
                            </Alert>
                        ) : null}
                        {!query.isError && view?.pending_count === 0 ? (
                            <p className="py-8 text-center text-sm text-muted-foreground">
                                当前没有待确认的资料更新
                            </p>
                        ) : null}
                        {view?.fields.map((difference) => (
                            <SourceDifference
                                key={`${view.product_version}-${view.source_version}-${difference.field}-${difference.sku_id ?? "product"}`}
                                difference={difference}
                                disabled={disabled}
                                pending={mutation.isPending}
                                onResolve={resolve}
                            />
                        ))}
                    </div>
                    <DialogFooter className="flex-wrap">
                        <Button
                            id="product-source-diff-refresh"
                            type="button"
                            variant="ghost"
                            disabled={query.isFetching || mutation.isPending}
                            onClick={() => void refresh()}
                        >
                            <RefreshCwIcon />
                            重新读取差异
                        </Button>
                        <Button
                            id="product-source-diff-dismiss"
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending}
                            onClick={() => setOpen(false)}
                        >
                            关闭
                        </Button>
                        {view?.pending_count ? (
                            <>
                                <Button
                                    id="product-source-diff-keep-all"
                                    type="button"
                                    variant="outline"
                                    disabled={disabled || mutation.isPending}
                                    onClick={() =>
                                        void resolve(
                                            view.fields.map((field) => ({
                                                field: field.field,
                                                ...(field.sku_id
                                                    ? { sku_id: field.sku_id }
                                                    : {}),
                                                action: "KEEP",
                                            })),
                                        )
                                    }
                                >
                                    全部保留公司版本
                                </Button>
                                <Button
                                    id="product-source-diff-adopt-all"
                                    type="button"
                                    disabled={
                                        disabled ||
                                        mutation.isPending ||
                                        !allAdoptable
                                    }
                                    onClick={() =>
                                        void resolve(
                                            view.fields.map((field) => ({
                                                field: field.field,
                                                ...(field.sku_id
                                                    ? { sku_id: field.sku_id }
                                                    : {}),
                                                action: "ADOPT",
                                            })),
                                        )
                                    }
                                >
                                    全部采用供应商版本
                                </Button>
                            </>
                        ) : null}
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    )
}
