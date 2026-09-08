"use client"

import { KeyRoundIcon } from "lucide-react"

import { Spinner } from "@/components/ui/spinner"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
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
    optionsError,
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
    optionsError: unknown
    value: string
    onValueChange: (value: string) => void
    allowed: boolean
    pending: boolean
    onSubmit: () => Promise<void>
}) {
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
                    <Label htmlFor={inputId}>
                        {kind === "credential" ? "密钥配置" : "地址配置"}
                    </Label>
                    <OpaqueReferenceSearchCombobox
                        kind={kind}
                        id={inputId}
                        value={value || null}
                        onValueChange={(v) => {
                            if (v) onValueChange(v)
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
                    <Button
                        id={`supplier-api-connections-reference-bind-${kind}-confirm`}
                        type="button"
                        disabled={!allowed || !value || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : (
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
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
