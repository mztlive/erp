"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    useDeleteDeliveryTemplateMutation,
    useSupplierDeliveryTemplatesQuery,
} from "../../hooks/queries"
import { ruleSetSummary } from "../../lib/delivery-template-summary"
import type { DeliveryTemplateView } from "../../types"
import { DeliveryTemplateEditorDialog } from "./delivery-template-editor-dialog"

/** 按供应商维护配送模板。 */
export function DeliveryTemplatesDialog({
    open,
    onOpenChange,
    initialSupplierId,
    canEdit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    initialSupplierId?: string
    canEdit: boolean
}) {
    const [supplierId, setSupplierId] = React.useState(initialSupplierId ?? "")
    const [editing, setEditing] = React.useState<{
        template: DeliveryTemplateView | null
        key: string
    } | null>(null)
    const [failure, setFailure] = React.useState<string | null>(null)
    const query = useSupplierDeliveryTemplatesQuery(supplierId)
    const remove = useDeleteDeliveryTemplateMutation()
    const templates = query.data ?? []
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90svh] overflow-y-auto sm:max-w-3xl"
                closeButtonId="delivery-templates-close"
            >
                <DialogHeader>
                    <DialogTitle>配送模板</DialogTitle>
                    <DialogDescription>
                        按供应商维护配送政策。供给未指定模板时跟随供应商默认模板；都没有时按全国配送、包邮判定。
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-wrap items-end justify-between gap-3">
                    <div className="grid min-w-64 flex-1 gap-1.5">
                        <Label htmlFor="delivery-templates-supplier">
                            供应商
                        </Label>
                        <SupplierSearchCombobox
                            id="delivery-templates-supplier"
                            value={supplierId || undefined}
                            onValueChange={(id) => {
                                setSupplierId(id ?? "")
                                setFailure(null)
                            }}
                            placeholder="选择供应商"
                        />
                    </div>
                    {canEdit ? (
                        <Button
                            id="delivery-templates-create"
                            type="button"
                            disabled={!supplierId}
                            onClick={() =>
                                setEditing({
                                    template: null,
                                    key: crypto.randomUUID(),
                                })
                            }
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            新建模板
                        </Button>
                    ) : null}
                </div>
                {!supplierId ? (
                    <p className="text-sm text-muted-foreground">
                        选择供应商后查看其配送模板。
                    </p>
                ) : query.isPending ? (
                    <div
                        className="h-24 animate-pulse rounded-lg bg-muted"
                        aria-busy="true"
                        aria-label="正在加载配送模板"
                    />
                ) : query.isError ? (
                    <p role="alert" className="text-sm text-destructive">
                        {getErrorMessage(
                            query.error,
                            "配送模板加载失败，请重试。",
                        )}
                    </p>
                ) : templates.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        该供应商还没有配送模板，其供给按全国配送、包邮判定。
                    </p>
                ) : (
                    <ul className="divide-y rounded-lg border">
                        {templates.map((template) => {
                            const id = `delivery-templates-${toAutomationIdSegment(template.id)}`
                            return (
                                <li
                                    key={template.id}
                                    className="space-y-1.5 p-3"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <span className="flex items-center gap-2 font-medium">
                                            {template.name}
                                            {template.is_default ? (
                                                <Badge variant="info">
                                                    默认
                                                </Badge>
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
                                                        setEditing({
                                                            template,
                                                            key: `${template.id}-${template.version}`,
                                                        })
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
                                                    onClick={async () => {
                                                        setFailure(null)
                                                        try {
                                                            await remove.mutateAsync(
                                                                {
                                                                    id: template.id,
                                                                    expectedVersion:
                                                                        template.version,
                                                                },
                                                            )
                                                        } catch (error) {
                                                            setFailure(
                                                                getErrorMessage(
                                                                    error,
                                                                    "删除失败，请稍后重试。",
                                                                ),
                                                            )
                                                        }
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
                                </li>
                            )
                        })}
                    </ul>
                )}
                {failure ? (
                    <p role="alert" className="text-sm text-destructive">
                        {failure}
                    </p>
                ) : null}
                {editing ? (
                    <DeliveryTemplateEditorDialog
                        key={editing.key}
                        supplierId={supplierId}
                        template={editing.template}
                        open
                        onOpenChange={(next) => {
                            if (!next) setEditing(null)
                        }}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
