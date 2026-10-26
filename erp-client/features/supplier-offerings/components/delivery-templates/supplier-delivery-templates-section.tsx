"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import { FormalActionConfirmDialog } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    useDeleteDeliveryTemplateMutation,
    useSupplierDeliveryTemplatesQuery,
} from "../../hooks/queries"
import { ruleSetSummary } from "../../lib/delivery-template-summary"
import type { DeliveryTemplateView } from "../../types"
import { DeliveryRulesPreview } from "./delivery-rules-preview"
import { DeliveryTemplateEditorDialog } from "./delivery-template-editor-dialog"

/** 供应商详情内独立维护配送模板，不提交供应商资料修订。 */
export function SupplierDeliveryTemplatesSection({
    supplierId,
    canEdit,
}: {
    supplierId: string
    canEdit: boolean
}) {
    const [editing, setEditing] = React.useState<{
        template: DeliveryTemplateView | null
    } | null>(null)
    const [deleting, setDeleting] = React.useState<DeliveryTemplateView | null>(
        null,
    )
    const query = useSupplierDeliveryTemplatesQuery(supplierId)
    const remove = useDeleteDeliveryTemplateMutation()
    const templates = query.data ?? []
    return (
        <section
            className="space-y-4"
            aria-labelledby="supplier-delivery-templates-title"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                    <h2
                        id="supplier-delivery-templates-title"
                        className="text-base font-semibold"
                    >
                        配送模板
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        供给未指定模板时跟随供应商默认模板；都没有时按全国配送、包邮判定。
                    </p>
                    <p className="text-sm text-muted-foreground">
                        模板单独保存并立即生效，无需保存供应商资料。修改模板会影响使用该模板的供给，调整默认模板会影响跟随默认的供给。
                    </p>
                </div>
                {canEdit ? (
                    <Button
                        id="delivery-templates-create"
                        type="button"
                        onClick={() => setEditing({ template: null })}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden />
                        新建模板
                    </Button>
                ) : null}
            </div>
            {query.isPending ? (
                <div
                    className="h-24 animate-pulse rounded-lg bg-muted"
                    aria-busy="true"
                    aria-label="正在加载配送模板"
                />
            ) : query.isError ? (
                <div className="space-y-2">
                    <p role="alert" className="text-sm text-destructive">
                        {getErrorMessage(
                            query.error,
                            "配送模板加载失败，请重试。",
                        )}
                    </p>
                    <Button
                        id="delivery-templates-retry"
                        type="button"
                        variant="outline"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : templates.length === 0 ? (
                <p className="rounded-lg border border-dashed p-4 text-sm text-muted-foreground">
                    该供应商还没有配送模板，其供给按全国配送、包邮判定。
                </p>
            ) : (
                <ul className="divide-y rounded-lg border">
                    {templates.map((template) => {
                        const id = `delivery-templates-${toAutomationIdSegment(template.id)}`
                        return (
                            <li key={template.id} className="space-y-2 p-4">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <span className="flex items-center gap-2 font-medium">
                                        {template.name}
                                        {template.is_default ? (
                                            <Badge variant="info">默认</Badge>
                                        ) : null}
                                    </span>
                                    {canEdit ? (
                                        <span className="flex gap-1">
                                            <Button
                                                id={`${id}-edit`}
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                onClick={() =>
                                                    setEditing({ template })
                                                }
                                            >
                                                修改
                                            </Button>
                                            <Button
                                                id={`${id}-delete`}
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                disabled={remove.isPending}
                                                onClick={() => {
                                                    setDeleting(template)
                                                }}
                                            >
                                                删除
                                            </Button>
                                        </span>
                                    ) : null}
                                </div>
                                <p className="text-sm text-muted-foreground">
                                    代发：
                                    {ruleSetSummary(
                                        "dropship",
                                        template.dropship,
                                    )}
                                </p>
                                <p className="text-sm text-muted-foreground">
                                    集采：
                                    {ruleSetSummary("bulk", template.bulk)}
                                </p>
                                <details>
                                    <summary
                                        id={`${id}-rules`}
                                        className="cursor-pointer text-sm"
                                    >
                                        查看配送规则
                                    </summary>
                                    <div className="pt-3">
                                        <DeliveryRulesPreview
                                            template={template}
                                        />
                                    </div>
                                </details>
                            </li>
                        )
                    })}
                </ul>
            )}
            {editing && canEdit ? (
                <DeliveryTemplateEditorDialog
                    supplierId={supplierId}
                    template={editing.template}
                    open
                    onOpenChange={(open) => {
                        if (!open) setEditing(null)
                    }}
                />
            ) : null}
            {deleting && canEdit ? (
                <FormalActionConfirmDialog
                    idPrefix="delivery-template-delete"
                    open
                    onOpenChange={(open) => {
                        if (!open && !remove.isPending) setDeleting(null)
                    }}
                    title="删除配送模板"
                    description={`确认删除「${deleting.name}」？被供给引用的模板不能删除。`}
                    actionLabel="删除模板"
                    cancelLabel="取消"
                    actionVariant="destructive"
                    fromStatus={{ label: "已保存", tone: "neutral" }}
                    toStatus={{ label: "已删除", tone: "destructive" }}
                    showStatusChange={false}
                    effects={
                        deleting.is_default
                            ? [
                                  "删除默认模板后，未指定模板的供给将按全国配送、包邮判定。",
                              ]
                            : []
                    }
                    pending={remove.isPending}
                    onConfirm={async () => {
                        await remove.mutateAsync({
                            id: deleting.id,
                            expectedVersion: deleting.version,
                        })
                        setDeleting(null)
                    }}
                />
            ) : null}
        </section>
    )
}
