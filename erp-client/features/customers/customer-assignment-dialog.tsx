"use client"

import * as React from "react"

import { OwnerCombobox } from "@/components/business"
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
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { useApplyCustomerAssignmentMutation } from "@/features/customers/queries"
import type { CustomerAssignmentView } from "@/features/customers/types"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import type { ApiError } from "@/lib/api"

/** 返回本地业务日期。 */
function todayBusinessDate(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 返回给定业务日的下一天。 */
function nextBusinessDate(value: string): string {
    const date = new Date(`${value}T00:00:00`)
    date.setDate(date.getDate() + 1)
    const pad = (part: number) => String(part).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 从统一 API 错误中提取服务端业务消息。 */
function mutationMessage(error: unknown): string {
    const apiError = error as ApiError
    const response = apiError.responseData as
        | { errorMessage?: string }
        | undefined
    return response?.errorMessage || apiError.message || "归属调整失败，请重试"
}

/** 建立/换任责任归属，或结束一条当前协作归属。 */
export function CustomerAssignmentDialog({
    customerId,
    open,
    target,
    onOpenChange,
}: {
    customerId: string
    open: boolean
    target?: CustomerAssignmentView
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useApplyCustomerAssignmentMutation()
    const owners = useOwnerOptionsQuery()
    const [userId, setUserId] = React.useState("")
    const [role, setRole] = React.useState<"OWNER" | "COLLABORATOR">(
        "COLLABORATOR",
    )
    const [effectiveFrom, setEffectiveFrom] = React.useState("")
    const [effectiveTo, setEffectiveTo] = React.useState("")
    const [reason, setReason] = React.useState("")
    const [validation, setValidation] = React.useState<string>()

    React.useEffect(() => {
        if (!open) return
        const today = todayBusinessDate()
        setUserId("")
        setRole("COLLABORATOR")
        setEffectiveFrom(today)
        setEffectiveTo(
            target
                ? target.effectiveFrom >= today
                    ? nextBusinessDate(today)
                    : today
                : "",
        )
        setReason("")
        setValidation(undefined)
        mutation.reset()
    }, [open, target]) // eslint-disable-line react-hooks/exhaustive-deps -- mutation identity is not reset state

    const ending = target != null

    const submit = (event: React.FormEvent<HTMLFormElement>) => {
        event.preventDefault()
        if (!reason.trim()) {
            setValidation("请填写调整原因")
            return
        }
        if (ending) {
            if (!effectiveTo || effectiveTo <= target.effectiveFrom) {
                setValidation(`结束日期必须晚于 ${target.effectiveFrom}`)
                return
            }
            mutation.mutate(
                {
                    customerId,
                    action: "end",
                    effectiveTo,
                    assignmentId: target.id,
                    version: target.version,
                    changeReason: reason,
                },
                { onSuccess: () => onOpenChange(false) },
            )
        } else {
            if (!userId) {
                setValidation("请选择销售人员")
                return
            }
            if (
                !effectiveFrom ||
                (effectiveTo && effectiveTo <= effectiveFrom)
            ) {
                setValidation("结束日期必须晚于生效日期")
                return
            }
            mutation.mutate(
                {
                    customerId,
                    action: "assign",
                    userId,
                    role,
                    effectiveFrom,
                    effectiveTo: effectiveTo || undefined,
                    changeReason: reason,
                },
                { onSuccess: () => onOpenChange(false) },
            )
        }
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>
                        {ending ? "结束协作归属" : "调整客户归属"}
                    </DialogTitle>
                    <DialogDescription>
                        {ending
                            ? "结束日期当日起不再计入协作范围，历史责任关系保留。"
                            : "换任负责人会结束重叠的旧负责人归属；新增协作不会改变负责人。"}
                    </DialogDescription>
                </DialogHeader>
                <form className="space-y-4" onSubmit={submit}>
                    {ending ? (
                        <div className="rounded-lg bg-muted px-3 py-2 text-sm">
                            {target.userName} · 协作销售 ·{" "}
                            {target.effectiveFrom} 起
                        </div>
                    ) : (
                        <>
                            <div className="space-y-2">
                                <Label htmlFor="customer-assignment-role">
                                    责任角色
                                </Label>
                                <NativeSelect
                                    id="customer-assignment-role"
                                    className="w-full"
                                    value={role}
                                    onChange={(event) =>
                                        setRole(
                                            event.target.value as
                                                | "OWNER"
                                                | "COLLABORATOR",
                                        )
                                    }
                                >
                                    <NativeSelectOption value="OWNER">
                                        负责销售
                                    </NativeSelectOption>
                                    <NativeSelectOption value="COLLABORATOR">
                                        协作销售
                                    </NativeSelectOption>
                                </NativeSelect>
                            </div>
                            <div className="space-y-2">
                                <Label>销售人员</Label>
                                <OwnerCombobox
                                    owners={owners.data ?? []}
                                    value={userId || undefined}
                                    onValueChange={(value) =>
                                        setUserId(value ?? "")
                                    }
                                />
                            </div>
                            <div className="grid gap-3 sm:grid-cols-2">
                                <div className="space-y-2">
                                    <Label htmlFor="customer-assignment-from">
                                        生效日期
                                    </Label>
                                    <Input
                                        id="customer-assignment-from"
                                        type="date"
                                        value={effectiveFrom}
                                        onChange={(event) =>
                                            setEffectiveFrom(event.target.value)
                                        }
                                    />
                                </div>
                                <div className="space-y-2">
                                    <Label htmlFor="customer-assignment-to">
                                        结束日期（可选）
                                    </Label>
                                    <Input
                                        id="customer-assignment-to"
                                        type="date"
                                        value={effectiveTo}
                                        onChange={(event) =>
                                            setEffectiveTo(event.target.value)
                                        }
                                    />
                                </div>
                            </div>
                        </>
                    )}
                    {ending ? (
                        <div className="space-y-2">
                            <Label htmlFor="customer-assignment-end-date">
                                结束日期
                            </Label>
                            <Input
                                id="customer-assignment-end-date"
                                type="date"
                                value={effectiveTo}
                                onChange={(event) =>
                                    setEffectiveTo(event.target.value)
                                }
                            />
                        </div>
                    ) : null}
                    <div className="space-y-2">
                        <Label htmlFor="customer-assignment-reason">
                            调整原因
                        </Label>
                        <Textarea
                            id="customer-assignment-reason"
                            value={reason}
                            onChange={(event) => setReason(event.target.value)}
                            placeholder="说明换任、协作或结束原因"
                        />
                    </div>
                    {validation || mutation.isError ? (
                        <p className="text-sm text-destructive" role="alert">
                            {validation || mutationMessage(mutation.error)}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                            disabled={mutation.isPending}
                        >
                            取消
                        </Button>
                        <Button type="submit" disabled={mutation.isPending}>
                            {mutation.isPending
                                ? "提交中…"
                                : ending
                                  ? "确认结束"
                                  : "确认调整"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
