"use client"

import * as React from "react"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import { useCustomerConflictQuery } from "@/features/customers/hooks/queries"
import type { CustomerCenterView } from "@/features/customers/types"
import { buildDefaults, type FormValues } from "./customer-form-values"

const fields = [
    ["legalName", "法定名称"],
    ["shortName", "客户简称"],
    ["unifiedCreditCode", "统一社会信用代码"],
    ["defaultPaymentTerm", "默认付款条件"],
    ["status", "客户状态"],
    ["contacts", "联系人"],
    ["addresses", "地址"],
    ["bankAccounts", "银行账户"],
] as const
type Field = (typeof fields)[number][0]
type Choice = "mine" | "latest"

function same(left: unknown, right: unknown) {
    return JSON.stringify(left) === JSON.stringify(right)
}

/** 只呈现可识别的业务字段，不输出内部行身份或敏感字段令牌。 */
function display(
    value: FormValues[Field],
    field: Field,
    profile: CustomerCenterView | null | undefined,
): string {
    if (typeof value === "string") {
        if (field === "status") return value === "active" ? "启用" : "停用"
        if (field === "defaultPaymentTerm")
            return (
                PAYMENT_TERM_OPTIONS.find((option) => option.value === value)
                    ?.label ||
                value ||
                "—"
            )
        return value || "—"
    }
    if (!value.length) return "无"
    return value
        .map((row) => {
            const masked =
                field === "contacts"
                    ? profile?.contacts.find(
                          (item) => item.id === row.existingId,
                      )?.phoneMasked
                    : field === "addresses"
                      ? profile?.addresses.find(
                            (item) => item.id === row.existingId,
                        )?.addressMasked
                      : profile?.bankAccounts.find(
                            (item) => item.id === row.existingId,
                        )?.accountMasked
            const hasSensitiveInput =
                "phone" in row
                    ? row.phone
                    : "address" in row
                      ? row.address
                      : row.accountNumber
            const content = Object.entries(row)
                .filter(
                    ([key, content]) =>
                        !["existingId", "draftId", "isDefault"].includes(key) &&
                        typeof content === "string" &&
                        content,
                )
                .map(([, content]) => content)
                .join(" · ")
            return (
                [content, !hasSensitiveInput ? masked : undefined]
                    .filter(Boolean)
                    .join(" · ") + (row.isDefault ? "（默认）" : "")
            )
        })
        .join("\n")
}

/** 冲突核对使用独立查询；读取最新资料不改动编辑表单或原始基线。 */
export function CustomerConflictDialog({
    open,
    onOpenChange,
    baseline,
    draft,
    onResolved,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    baseline: CustomerCenterView
    draft: FormValues
    onResolved: (latest: CustomerCenterView, values: FormValues) => void
}) {
    const query = useCustomerConflictQuery(baseline.customerId, open)
    const [choices, setChoices] = React.useState<
        Partial<Record<Field, Choice>>
    >({})
    const version = query.data
        ? `${query.data.lockVersion}:${query.data.partyLockVersion}`
        : ""
    React.useEffect(() => {
        setChoices({})
    }, [open, version])
    const initial = buildDefaults("edit", baseline)
    const latest = query.data ? buildDefaults("edit", query.data) : null
    const changed = latest
        ? fields.filter(
              ([field]) =>
                  !same(initial[field], draft[field]) ||
                  !same(initial[field], latest[field]),
          )
        : []
    const choice = (field: Field): Choice | undefined => {
        if (!latest) return undefined
        const mineChanged = !same(initial[field], draft[field])
        const serverChanged = !same(initial[field], latest[field])
        if (!mineChanged || same(draft[field], latest[field])) return "latest"
        if (!serverChanged) return "mine"
        return choices[field]
    }
    const blockedGroups = latest
        ? (["contacts", "addresses", "bankAccounts"] as const).filter(
              (field) => {
                  if (choice(field) !== "mine") return false
                  const currentIds = new Set(
                      latest[field].map((row) => row.existingId),
                  )
                  return draft[field].some(
                      (row) =>
                          row.existingId &&
                          !currentIds.has(row.existingId) &&
                          !("phone" in row
                              ? row.phone.trim() && !row.phone.includes("*")
                              : "address" in row
                                ? row.address.trim() &&
                                  !row.address.includes("*")
                                : row.accountNumber.trim() &&
                                  !row.accountNumber.includes("*")),
                  )
              },
          )
        : []
    const ready =
        blockedGroups.length === 0 &&
        Boolean(
            query.data &&
            latest &&
            !query.isFetching &&
            !query.isError &&
            query.data.allowedActions.includes("EDIT_CUSTOMER") &&
            changed.every(([field]) => choice(field)),
        )

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="customers-conflict-close"
                className="max-h-[85vh] overflow-y-auto sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>核对客户资料修改</DialogTitle>
                    <DialogDescription>
                        其他人已更新资料。你的输入仍然保留，请核对差异后选择保留内容。联系人、地址和银行账户分别按整组核对。
                    </DialogDescription>
                </DialogHeader>
                {query.isFetching ? (
                    <p className="text-sm text-muted-foreground">
                        正在读取最新资料…
                    </p>
                ) : null}
                {query.isError ||
                (!query.isPending && !query.isFetching && !query.data) ? (
                    <div role="alert" className="space-y-2 text-sm">
                        <p>
                            {query.isError
                                ? getErrorMessage(
                                      query.error,
                                      "读取最新资料失败，你的输入仍已保留。",
                                  )
                                : "当前无法读取此客户，请联系有权限的同事核对。你的输入仍已保留。"}
                        </p>
                        <Button
                            id="customers-conflict-retry"
                            type="button"
                            variant="outline"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : null}
                {query.data &&
                !query.data.allowedActions.includes("EDIT_CUSTOMER") ? (
                    <p role="alert" className="text-sm">
                        当前已无客户资料修改权限。你的输入仍已保留，请联系有权限的同事处理。
                    </p>
                ) : null}
                {blockedGroups.length ? (
                    <p role="alert" className="text-sm text-destructive">
                        你选择保留的部分联系人、地址或银行账户已被其他人替换，无法直接沿用原来的敏感资料。请选择采用最新内容，或返回表单补充需要重新保存的手机号、地址或账号；已输入内容仍然保留。
                    </p>
                ) : null}
                {latest && !query.isError ? (
                    <div className="space-y-4">
                        {changed.length === 0 ? (
                            <p className="text-sm">
                                资料内容没有差异，可以采用最新版本继续编辑。
                            </p>
                        ) : null}
                        {changed.map(([field, label]) => (
                            <div
                                key={field}
                                className="space-y-2 rounded-md border p-3"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <span className="text-sm font-medium">
                                        {label}
                                    </span>
                                    {!same(initial[field], draft[field]) &&
                                    !same(initial[field], latest[field]) &&
                                    !same(draft[field], latest[field]) ? (
                                        <Select
                                            value={choices[field] ?? ""}
                                            onValueChange={(value) => {
                                                if (
                                                    value === "mine" ||
                                                    value === "latest"
                                                )
                                                    setChoices((current) => ({
                                                        ...current,
                                                        [field]: value,
                                                    }))
                                            }}
                                        >
                                            <SelectTrigger
                                                id={`customers-conflict-choice-${field.toLowerCase()}`}
                                                aria-label={`${label}保留内容`}
                                                className="w-48"
                                            >
                                                <SelectValue placeholder="请选择保留内容" />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem
                                                    id={`customers-conflict-latest-${field.toLowerCase()}`}
                                                    value="latest"
                                                >
                                                    采用最新内容
                                                </SelectItem>
                                                <SelectItem
                                                    id={`customers-conflict-mine-${field.toLowerCase()}`}
                                                    value="mine"
                                                >
                                                    保留我的修改
                                                </SelectItem>
                                            </SelectContent>
                                        </Select>
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            {choice(field) === "mine"
                                                ? "保留我的修改"
                                                : "采用最新内容"}
                                        </span>
                                    )}
                                </div>
                                <div className="grid gap-3 text-sm sm:grid-cols-3">
                                    {[
                                        ["打开时", initial[field]],
                                        ["当前最新", latest[field]],
                                        ["我的输入", draft[field]],
                                    ].map(([title, value]) => (
                                        <div key={String(title)}>
                                            <p className="mb-1 text-xs text-muted-foreground">
                                                {String(title)}
                                            </p>
                                            <p className="whitespace-pre-wrap break-words">
                                                {display(
                                                    value as FormValues[Field],
                                                    field,
                                                    title === "当前最新"
                                                        ? query.data
                                                        : baseline,
                                                )}
                                            </p>
                                        </div>
                                    ))}
                                </div>
                            </div>
                        ))}
                    </div>
                ) : null}
                <DialogFooter>
                    <Button
                        id="customers-conflict-keep-editing"
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        返回，保留输入
                    </Button>
                    <Button
                        id="customers-conflict-apply"
                        type="button"
                        disabled={!ready}
                        onClick={() => {
                            if (!query.data || !latest || !ready) return
                            const merged = {
                                ...latest,
                                changeReason: draft.changeReason,
                            }
                            for (const [field] of fields) {
                                if (choice(field) === "mine")
                                    Object.assign(merged, {
                                        [field]: draft[field],
                                    })
                            }
                            for (const field of [
                                "contacts",
                                "addresses",
                                "bankAccounts",
                            ] as const) {
                                const currentIds = new Set(
                                    latest[field].map((row) => row.existingId),
                                )
                                Object.assign(merged, {
                                    [field]: merged[field].map((row) =>
                                        row.existingId &&
                                        !currentIds.has(row.existingId)
                                            ? { ...row, existingId: undefined }
                                            : row,
                                    ),
                                })
                            }
                            onResolved(query.data, merged)
                        }}
                    >
                        确认核对，继续编辑
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
