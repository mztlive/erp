"use client"

import * as React from "react"

import {
    BusinessStatusBadge,
    DetailInlineTable,
    DetailWorkspaceSection,
} from "@/components/business"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
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
    noteMutation: Parameters<
        typeof useSupplierOrderCenterNoteForm
    >[0]["noteMutation"]
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
}) {
    const profile = useAccountProfileQuery()
    const canAddNote =
        hasPermission(
            profile.data?.permissions,
            "supplier_fulfillment_order:investigate",
        ) ||
        hasPermission(
            profile.data?.permissions,
            "supplier_fulfillment_order:submit",
        )
    const noteForm = useSupplierOrderCenterNoteForm({
        orderId,
        detail,
        noteMutation,
        setResult,
    })

    return (
        <>
            <DetailWorkspaceSection title="动作与审计" width="full">
                <DetailInlineTable frame="scroll">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>动作</TableHead>
                                <TableHead>结果</TableHead>
                                <TableHead>操作人</TableHead>
                                <TableHead>时间</TableHead>
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
                                    <TableCell className="text-xs">
                                        {a.actor}
                                    </TableCell>
                                    <TableCell className="num text-xs">
                                        {formatDateTime(
                                            a.at,
                                            "fullIntl",
                                            "passthrough",
                                        )}
                                    </TableCell>
                                    <TableCell className="num">
                                        {a.attemptCount}
                                    </TableCell>
                                </TableRow>
                            ))}
                            {detail.actions.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={5}
                                        className="py-6 text-center text-body-compact text-muted-foreground"
                                    >
                                        暂无动作记录
                                    </TableCell>
                                </TableRow>
                            ) : null}
                        </TableBody>
                    </Table>
                </DetailInlineTable>
            </DetailWorkspaceSection>

            {canAddNote ? (
                <DetailWorkspaceSection title="记录协同说明" width="form">
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void noteForm.handleSubmit()
                        }}
                    >
                        <noteForm.AppField
                            name="comment"
                            children={(field) => {
                                const isInvalid =
                                    field.state.meta.isTouched &&
                                    !field.state.meta.isValid
                                const errors = field.state.meta.errors.map(
                                    (error) => ({
                                        message:
                                            typeof error === "string"
                                                ? error
                                                : error?.message,
                                    }),
                                )
                                return (
                                    <Field
                                        data-invalid={isInvalid || undefined}
                                    >
                                        <FieldLabel htmlFor="supplier-order-center-audit-comment">
                                            协同说明
                                            <RequiredMark />
                                        </FieldLabel>
                                        <Textarea
                                            id="supplier-order-center-audit-comment"
                                            value={field.state.value}
                                            disabled={noteMutation.isPending}
                                            aria-invalid={
                                                isInvalid || undefined
                                            }
                                            aria-describedby={
                                                isInvalid
                                                    ? "supplier-order-center-audit-comment-error"
                                                    : undefined
                                            }
                                            onChange={(e) =>
                                                field.handleChange(
                                                    e.target.value,
                                                )
                                            }
                                            onBlur={field.handleBlur}
                                            placeholder="不改变订单状态，仅追加审计说明"
                                            rows={3}
                                        />
                                        {isInvalid ? (
                                            <FieldError
                                                id="supplier-order-center-audit-comment-error"
                                                errors={errors}
                                            />
                                        ) : null}
                                    </Field>
                                )
                            }}
                        />
                        <noteForm.AppForm>
                            <noteForm.SubmitButton
                                id="supplier-order-center-audit-submit"
                                label="提交说明"
                                loading={noteMutation.isPending}
                            />
                        </noteForm.AppForm>
                    </form>
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}
