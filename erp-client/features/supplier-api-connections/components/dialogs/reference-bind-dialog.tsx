"use client"

import { useState } from "react"
import { useOpaqueReferenceOptionsQuery } from "@/features/supplier-api-connections/hooks/use-opaque-reference-options"

import { KeyRoundIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { OpaqueReferenceSearchCombobox } from "@/features/supplier-api-connections/components/opaque-reference-search-combobox"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { REFERENCE_STATE_LABEL } from "@/features/supplier-api-connections/types"
import { getErrorMessage } from "@/lib/api/errors"

/** 密钥/地址不透明引用选择器；页面不接触引用正文。 */
export function ReferenceBindDialog({
    open,
    onOpenChange,
    kind,
    conn,
    value,
    onValueChange,
    allowed,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    kind: "credential" | "endpoint"
    conn: ConnectionCenterView
    value: string
    onValueChange: (value: string) => void
    allowed: boolean
    pending: boolean
    onSubmit: () => Promise<void>
}) {
    const optionsQuery = useOpaqueReferenceOptionsQuery(
        conn.connectionId,
        kind,
        conn.version,
        open && allowed,
    )
    const [selectionExpired, setSelectionExpired] = useState(false)
    const selected = optionsQuery.data?.find(
        (option) => option.referenceId === value,
    )
    const submit = async () => {
        if (!selected || selected.expiresAt * 1000 <= Date.now()) {
            setSelectionExpired(true)
            onValueChange("")
            await optionsQuery.refetch()
            return
        }
        await onSubmit()
    }
    const optionsError = optionsQuery.isError ? optionsQuery.error : undefined
    const isProd = conn.environment === "PRODUCTION"
    const kindLabel = kind === "credential" ? "密钥配置" : "地址配置"
    const ref =
        kind === "credential"
            ? conn.safeReferences.credential
            : conn.safeReferences.endpoint
    const inputId =
        kind === "credential"
            ? "supplier-api-connections-reference-bind-credential-input"
            : "supplier-api-connections-reference-bind-endpoint-input"
    const errorFallback =
        kind === "credential"
            ? "无法取得密钥配置列表，请重试后再选择。"
            : "无法取得地址配置列表，请重试后再选择。"
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId={`supplier-api-connections-reference-bind-${kind}-close`}
            >
                <DialogHeader>
                    <DialogTitle>
                        {`${ref.state === "MISSING" ? "绑定" : "更换"}${isProd ? "生产环境" : ""}${kindLabel}`}
                    </DialogTitle>
                    <DialogDescription>
                        {kind === "credential"
                            ? "从已配置的密钥中选择。"
                            : "从已配置的接口地址中选择。"}
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    {optionsError ? (
                        <Alert variant="destructive" role="alert">
                            <AlertTitle>引用选项加载失败</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(optionsError, errorFallback)}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {selectionExpired ? (
                        <p role="alert" className="text-sm text-destructive">
                            配置选择已过期，请重新选择后保存。
                        </p>
                    ) : null}
                    <Label htmlFor={inputId}>
                        {kind === "credential" ? "密钥配置" : "地址配置"}
                    </Label>
                    <OpaqueReferenceSearchCombobox
                        options={optionsQuery.data ?? []}
                        loading={optionsQuery.isFetching}
                        emptyLabel="当前环境没有可选择的配置，请先登记对应的供应商技术参数。"
                        id={inputId}
                        value={value || null}
                        onValueChange={(v) => {
                            if (v) {
                                setSelectionExpired(false)
                                onValueChange(v)
                            }
                        }}
                        placeholder={
                            kind === "credential"
                                ? "选择密钥配置"
                                : "选择地址配置"
                        }
                        allowClear={false}
                    />
                    <p className="text-xs text-muted-foreground">
                        当前状态：
                        {REFERENCE_STATE_LABEL[ref.state]}
                        {ref.alias ? ` · ${ref.alias}` : ""}
                    </p>
                </div>
                <DialogFooter>
                    <Button
                        id={`supplier-api-connections-reference-bind-${kind}-cancel`}
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <LoadingButton
                        id={`supplier-api-connections-reference-bind-${kind}-confirm`}
                        loading={pending}
                        type="button"
                        disabled={
                            !allowed ||
                            !selected ||
                            pending ||
                            optionsQuery.isFetching
                        }
                        onClick={() => void submit()}
                    >
                        {!pending && (
                            <KeyRoundIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                        )}
                        {pending
                            ? "绑定中…"
                            : kind === "credential"
                              ? "保存密钥配置"
                              : "保存地址配置"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
