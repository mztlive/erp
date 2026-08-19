"use client"

import type { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"

type AllocationForm = ReturnType<typeof useAllocationSession>["form"]

/**
 * 核销记录表单。发票字段只有号码/金额/日期，不含审批流程选择。
 *
 * @param form 核销会话表单。
 * @param isReceipt 回款模式为 true，发票模式为 false。
 * @param existing 是否继续已有记录。
 * @param locked 已确认或已提交后只读。
 */
export function SessionFactFields({
    form,
    isReceipt,
    existing,
    locked,
}: {
    form: AllocationForm
    isReceipt: boolean
    existing: boolean
    locked: boolean
}) {
    return (
        <section className="space-y-3 rounded-2xl border bg-card p-4">
            <h3 className="text-sm font-semibold">
                {isReceipt ? "回款记录" : "销项发票记录"}
            </h3>
            <p className="text-xs text-muted-foreground">
                已确认记录不可编辑删除；此处仅用于新登记或继续核销。
            </p>
            {isReceipt ? (
                <div className="space-y-3">
                    <form.AppField
                        name="receivedAt"
                        children={(field) => (
                            <field.DateTimeField
                                label="实际到账时间"
                                disabled={locked}
                            />
                        )}
                    />
                    <form.AppField
                        name="amount"
                        children={(field) => (
                            <field.TextField
                                label={
                                    existing
                                        ? "未分配余额（可分配上限）"
                                        : "到账金额（含税）"
                                }
                                disabled={existing}
                            />
                        )}
                    />
                    <form.AppField
                        name="bankReference"
                        children={(field) => (
                            <field.TextField
                                label="银行流水/回单引用"
                                disabled={locked}
                            />
                        )}
                    />
                </div>
            ) : (
                <div className="space-y-3">
                    <form.AppField
                        name="invoiceCode"
                        children={(field) => (
                            <field.TextField
                                label="发票代码"
                                disabled={locked}
                            />
                        )}
                    />
                    <form.AppField
                        name="invoiceNo"
                        children={(field) => (
                            <field.TextField
                                label="发票号码"
                                disabled={locked}
                            />
                        )}
                    />
                    <form.AppField
                        name="invoiceDate"
                        children={(field) => (
                            <field.DateField
                                label="开票日期"
                                disabled={locked}
                            />
                        )}
                    />
                    <form.AppField
                        name="grossAmount"
                        children={(field) => (
                            <field.TextField
                                label={
                                    existing
                                        ? "未分配含税余额"
                                        : "含税金额"
                                }
                                disabled={existing}
                            />
                        )}
                    />
                    <div className="grid grid-cols-2 gap-2">
                        <form.AppField
                            name="netAmount"
                            children={(field) => (
                                <field.TextField
                                    label="不含税"
                                    disabled={locked}
                                />
                            )}
                        />
                        <form.AppField
                            name="taxAmount"
                            children={(field) => (
                                <field.TextField
                                    label="税额"
                                    disabled={locked}
                                />
                            )}
                        />
                    </div>
                </div>
            )}
        </section>
    )
}
