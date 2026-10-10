"use client"

import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { FieldLabel } from "@/components/ui/field"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { ImportCustomer } from "../api/import-identities"

type Props = {
    mode: "existing" | "create"
    recognizedName?: string | null
    matched: boolean
    createOnly: boolean
    candidates: ImportCustomer[]
    selectedCustomerId: string
    onSelectCustomer: (id: string) => void
    expectedCustomerId?: string
    disabled: boolean
    canCreate: boolean
    onModeChange: (mode: "existing" | "create") => void
    children: ReactNode
}

export function ContractImportCustomerChoice({
    mode,
    recognizedName,
    matched,
    createOnly,
    candidates,
    selectedCustomerId,
    onSelectCustomer,
    expectedCustomerId,
    disabled,
    canCreate,
    onModeChange,
    children,
}: Props) {
    const needsAttention = !recognizedName?.trim() || !matched
    return (
        <section
            className="min-w-0 space-y-4 rounded-lg border border-border p-4"
            aria-labelledby="contract-import-customer-heading"
        >
            <h4
                id="contract-import-customer-heading"
                className="text-sm font-semibold"
            >
                客户（对方签约主体）
            </h4>
            <div
                className={cn(
                    "space-y-1 rounded-lg border px-3 py-2.5",
                    needsAttention
                        ? "border-warning-border bg-warning-soft text-warning-soft-foreground"
                        : "border-transparent bg-muted/50",
                )}
            >
                <p
                    className={cn(
                        "text-xs",
                        needsAttention
                            ? "text-warning-soft-foreground/90"
                            : "text-muted-foreground",
                    )}
                >
                    合同识别名称
                </p>
                <p className="break-words text-sm font-medium">
                    {recognizedName || "未识别客户名称"}
                </p>
                <p
                    className={cn(
                        "text-xs leading-relaxed",
                        needsAttention
                            ? "text-warning-soft-foreground/90"
                            : "text-muted-foreground",
                    )}
                >
                    {expectedCustomerId
                        ? "客户已由当前业务确定，请核对合同签约信息。"
                        : matched
                          ? "已自动匹配客户，请与合同原文核对。"
                          : createOnly
                            ? canCreate
                                ? "当前可选范围内没有已有客户，请核对并补充建档信息。"
                                : "当前可选范围内没有已有客户，请联系有权限的同事建档或调整客户归属。"
                            : canCreate
                              ? "未找到完全匹配的客户，请手动选择或新建客户。"
                              : "未找到完全匹配的客户，请手动选择已有客户。"}
                </p>
            </div>
            {!expectedCustomerId && !createOnly ? (
                <RadioGroup
                    id="contract-import-customer-mode"
                    aria-label="客户处理方式"
                    value={mode}
                    disabled={disabled}
                    className="grid gap-3 sm:grid-cols-2"
                    onValueChange={(value) => {
                        if (value === "existing" || value === "create")
                            onModeChange(value)
                    }}
                >
                    {[
                        { value: "existing", label: "选择已有客户" },
                        { value: "create", label: "按合同信息建档" },
                    ].map((option) => (
                        <FieldLabel
                            key={option.value}
                            htmlFor={`contract-import-customer-mode-${option.value}`}
                            className={cn(
                                "flex min-h-11 w-full cursor-pointer items-center gap-3 rounded-lg border border-border px-3 py-2.5 font-medium has-focus-visible:ring-2 has-focus-visible:ring-ring",
                                mode === option.value && "bg-muted/60",
                                (disabled ||
                                    (option.value === "create" &&
                                        !canCreate)) &&
                                    "cursor-not-allowed opacity-50",
                            )}
                        >
                            <RadioGroupItem
                                id={`contract-import-customer-mode-${option.value}`}
                                value={option.value}
                                nativeButton
                                render={
                                    <button
                                        type="button"
                                        aria-label={option.label}
                                    />
                                }
                                disabled={
                                    disabled ||
                                    (option.value === "create" && !canCreate)
                                }
                            />
                            {option.label}
                        </FieldLabel>
                    ))}
                </RadioGroup>
            ) : null}
            {!expectedCustomerId && !createOnly && !canCreate ? (
                <p className="text-xs text-muted-foreground">
                    你没有创建客户的权限，请选择已有客户或联系有权限的同事建档。
                </p>
            ) : null}
            {!expectedCustomerId &&
            mode === "existing" &&
            candidates.length > 0 ? (
                <div className="space-y-2">
                    <p className="text-xs text-muted-foreground">
                        以下客户可能相关，请核对后选择；也可在下方搜索其他客户。
                    </p>
                    <div className="grid gap-2">
                        {candidates.map((candidate) => (
                            <Button
                                key={candidate.id}
                                id={`contract-import-customer-candidate-${toAutomationIdSegment(candidate.id)}`}
                                type="button"
                                variant="outline"
                                className="h-auto min-w-0 justify-start whitespace-normal px-3 py-2 text-left"
                                disabled={disabled}
                                aria-pressed={
                                    selectedCustomerId === candidate.id
                                }
                                onClick={() => onSelectCustomer(candidate.id)}
                            >
                                <span className="min-w-0 space-y-1">
                                    <span className="block break-words">
                                        {candidate.legalName}
                                    </span>
                                    <span className="block break-words text-xs font-normal text-muted-foreground">
                                        {candidate.customerNo} · 信用代码：
                                        {candidate.creditCode || "未登记"}
                                    </span>
                                </span>
                            </Button>
                        ))}
                    </div>
                </div>
            ) : null}
            {children}
        </section>
    )
}
