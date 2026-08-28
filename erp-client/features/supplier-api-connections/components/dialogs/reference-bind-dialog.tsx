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
    const kindLabel = kind === "credential" ? "密钥引用" : "地址引用"
    const ref =
        kind === "credential"
            ? conn.safeReferences.credential
            : conn.safeReferences.endpoint
    const inputId = kind === "credential" ? "opaque-ref" : "endpoint-ref"
    const errorFallback =
        kind === "credential"
            ? "无法取得密钥管理引用列表，请重试后再选择。"
            : "无法取得地址配置引用列表，请重试后再选择。"
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>
                        {isProd
                            ? `轮换生产环境${kindLabel}`
                            : `绑定/轮换${kindLabel}`}
                    </DialogTitle>
                    <DialogDescription>
                        {kind === "credential"
                            ? "只能从密钥管理系统选择不透明引用。无明文密钥输入框；页面、URL 与结果均不返回正文。"
                            : "只能从系统提供的地址配置引用中选择，不能自由输入地址。"}
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
                        {kind === "credential"
                            ? "密钥管理引用"
                            : "地址配置引用"}
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
                                ? "选择不透明引用"
                                : "选择地址配置引用"
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
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={!allowed || !value || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner className="size-4 animate-spin" aria-hidden="true" />
                        ) : (
                            <KeyRoundIcon className="size-4" aria-hidden="true" />
                        )}
                        {pending
                            ? "绑定中…"
                            : kind === "credential"
                              ? "确认绑定引用"
                              : "确认绑定地址"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
