"use client"

import * as React from "react"

import {
    BusinessStatusBadge,
    DocumentSection,
} from "@/components/business"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { useSupplierOrderCenterNoteForm } from "@/features/supplier-orders/hooks/use-supplier-order-center-forms"
import type { SupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

export function AuditSection({
    orderId,
    detail,
    noteMutation,
    setResult,
}: {
    orderId: string
    detail: SupplierOrderDetailView
    noteMutation: Parameters<typeof useSupplierOrderCenterNoteForm>[0]["noteMutation"]
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
}) {
    const noteForm = useSupplierOrderCenterNoteForm({
        orderId,
        detail,
        noteMutation,
        setResult,
    })

    return (
        <DocumentSection
            title="动作与审计"
            description="不展示密钥、完整消息内容或敏感地址"
        >
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>动作</TableHead>
                        <TableHead>结果</TableHead>
                        <TableHead>操作人</TableHead>
                        <TableHead>时间</TableHead>
                        <TableHead>任务号尾号</TableHead>
                        <TableHead>尝试</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {detail.actions.map((a) => (
                        <TableRow key={a.actionId}>
                            <TableCell>
                                <div>{a.actionLabel}</div>
                            </TableCell>
                            <TableCell>
                                <BusinessStatusBadge
                                    context="list"
                                    label={a.outcomeLabel}
                                    tone={a.outcomeTone}
                                />
                            </TableCell>
                            <TableCell className="text-xs">{a.actor}</TableCell>
                            <TableCell className="num text-xs">
                                {formatDateTime(
                                    a.at,
                                    "fullIntl",
                                    "passthrough",
                                )}
                            </TableCell>
                            <TableCell className="num text-xs">
                                {a.idempotencyKeyTail}
                            </TableCell>
                            <TableCell className="num">
                                {a.attemptCount}
                            </TableCell>
                        </TableRow>
                    ))}
                    {detail.actions.length === 0 ? (
                        <TableRow>
                            <TableCell
                                colSpan={6}
                                className="py-6 text-center text-sm text-muted-foreground"
                            >
                                暂无动作记录
                            </TableCell>
                        </TableRow>
                    ) : null}
                </TableBody>
            </Table>

            <Separator className="my-4" />
            <form
                className="space-y-2"
                onSubmit={(e) => {
                    e.preventDefault()
                    void noteForm.handleSubmit()
                }}
            >
                <Label htmlFor="collab-note">记录协同说明</Label>
                <noteForm.AppField
                    name="comment"
                    children={(field) => (
                        <Textarea
                            id="collab-note"
                            value={field.state.value}
                            onChange={(e) =>
                                field.handleChange(e.target.value)
                            }
                            onBlur={field.handleBlur}
                            placeholder="不改变订单状态，仅追加审计说明"
                            rows={3}
                        />
                    )}
                />
                <noteForm.AppForm>
                    <noteForm.SubmitButton label="提交说明" />
                </noteForm.AppForm>
            </form>
        </DocumentSection>
    )
}
