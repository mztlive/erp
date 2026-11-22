"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionConfirmDialog,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
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
                    <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
                        <h2
                            id="supplier-delivery-templates-title"
                            className="text-section-title font-semibold tracking-tight text-foreground"
                        >
                            配送模板
                        </h2>
                        {query.isSuccess ? (
                            <span className="num text-xs text-muted-foreground">
                                {templates.length} 个模板 · 独立实时保存
                            </span>
                        ) : null}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        供给未指定模板时跟随供应商默认模板；都没有时按全国配送、包邮判定。模板单独保存并立即生效，无需保存供应商资料。
                    </p>
                </div>
                {canEdit ? (
                    <Button
                        id="delivery-templates-create"
                        type="button"
                        size="sm"
                        onClick={() => setEditing({ template: null })}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        新建模板
                    </Button>
                ) : null}
            </div>
            {query.isPending ? (
                <div
                    className="h-24 animate-pulse rounded-md bg-muted"
                    aria-busy="true"
                    aria-label="正在加载配送模板"
                />
            ) : query.isError ? (
                <BusinessFailureState
                    title="配送模板加载失败"
                    error={query.error}
                    onRetry={() => {
                        void query.refetch()
                    }}
                />
            ) : templates.length === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无配送模板"
                    description="该供应商还没有配送模板，其供给按全国配送、包邮判定。"
                />
            ) : (
                <ul className="divide-y divide-border/60 border-y border-border/60">
                    {templates.map((template) => {
                        const id = `delivery-templates-${toAutomationIdSegment(template.id)}`
                        return (
                            <li
                                key={template.id}
                                className="space-y-2 py-4 first:pt-3.5 last:pb-3.5"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <span className="flex flex-wrap items-center gap-2 font-medium text-foreground">
                                        {template.name}
                                        {template.is_default ? (
                                            <StatusBadge
                                                tone="info"
                                                label="默认模板"
                                            />
                                        ) : null}
                                    </span>
                                    {canEdit ? (
                                        <span className="flex items-center gap-1">
                                            <Button
                                                id={`${id}-edit`}
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                className={
                                                    detailWorkspaceStyles.quietButton
                                                }
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
                                                className={
                                                    detailWorkspaceStyles.quietDangerButton
                                                }
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
                                <p className="text-xs text-muted-foreground">
                                    代发：
                                    {ruleSetSummary(
                                        "dropship",
                                        template.dropship,
                                    )}
                                </p>
                                <p className="text-xs text-muted-foreground">
                                    集采：
                                    {ruleSetSummary("bulk", template.bulk)}
                                </p>
                                <details>
                                    <summary
                                        id={`${id}-rules`}
                                        className="cursor-pointer text-body-compact text-primary hover:underline"
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
                        const deletedName = deleting.name
                        await remove.mutateAsync({
                            id: deleting.id,
                            expectedVersion: deleting.version,
                        })
                        setDeleting(null)
                        toast.add({
                            title: "已删除配送模板",
                            description: `模板「${deletedName}」已删除。`,
                            type: "success",
                        })
                    }}
                />
            ) : null}
        </section>
    )
}
