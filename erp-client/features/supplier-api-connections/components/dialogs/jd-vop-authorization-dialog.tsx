"use client"

import { useEffect, useState } from "react"
import { useStore } from "@tanstack/react-form"
import { EyeIcon, EyeOffIcon } from "lucide-react"
import { z } from "zod"
import { DiscardConfirmDialog } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAuthorizeJdVopAccount } from "../../hooks/jd-vop-authorization"
import { jdVopAuthorizationError } from "../../lib/jd-vop-authorization"
import { readJdVopPrivateKey } from "../../lib/jd-vop-private-key"
import type { JdVopAuthorizationStatus } from "../../types/jd-vop-authorization"
import { JdVopPrivateKeyField } from "./jd-vop-private-key-field"

function withinBytes(value: string, maximum: number) {
    return new TextEncoder().encode(value).byteLength <= maximum
}

export function JdVopAuthorizationDialog({
    connectionId,
    status,
    idPrefix,
    onClose,
    onSubmitted,
}: {
    connectionId: string
    status: JdVopAuthorizationStatus
    idPrefix: string
    onClose: () => void
    onSubmitted: (operationId: string, uncertain: boolean) => void
}) {
    const authorize = useAuthorizeJdVopAccount(connectionId)
    const [errorMessage, setErrorMessage] = useState<string | null>(null)
    const [showPassword, setShowPassword] = useState(false)
    const [discardOpen, setDiscardOpen] = useState(false)
    const recovering =
        status.state === "unknown" || status.operation_state === "sent_unknown"
    const form = useAppForm({
        defaultValues: {
            username: "",
            password: "",
            redirectUri: "",
            privateKeyFile: null as File | null,
            evidenceReference: "",
        },
        validators: {
            onSubmit: z.object({
                username: z
                    .string()
                    .trim()
                    .min(1, "请输入企业采购账号")
                    .refine(
                        (value) => withinBytes(value, 256),
                        "采购账号过长，请核对（最多 256 字节）",
                    ),
                password: z
                    .string()
                    .min(1, "请输入采购账号密码")
                    .refine(
                        (value) => withinBytes(value, 4096),
                        "密码过长，请核对（最多 4096 字节）",
                    ),
                redirectUri: z
                    .string()
                    .trim()
                    .url("请输入完整的应用回调地址")
                    .refine(
                        (value) => withinBytes(value, 2048),
                        "回调地址过长，请核对（最多 2048 字节）",
                    )
                    .refine(
                        (value) => /^https?:\/\//i.test(value),
                        "回调地址须以 http:// 或 https:// 开头",
                    ),
                privateKeyFile: z.custom<File | null>(
                    (file) => file instanceof File,
                    "请选择本次授权使用的 RSA 私钥文件",
                ),
                evidenceReference: z
                    .string()
                    .trim()
                    .refine(
                        (value) => withinBytes(value, 1024),
                        "核实依据不能超过 1024 字节",
                    )
                    .refine(
                        (value) => !recovering || value.length > 0,
                        "请填写原授权结果已核实的依据",
                    ),
            }),
        },
        onSubmit: async ({ value }) => {
            setErrorMessage(null)
            let sent = false
            let privateKey = ""
            const operationId = crypto.randomUUID()
            try {
                if (!value.privateKeyFile) return
                privateKey = await readJdVopPrivateKey(value.privateKeyFile)
                sent = true
                await authorize.submit({
                    operation_id: operationId,
                    expected_version: status.version,
                    username: value.username.trim(),
                    password: value.password,
                    redirect_uri: value.redirectUri.trim(),
                    private_key: privateKey,
                    ...(recovering
                        ? { evidence_reference: value.evidenceReference.trim() }
                        : {}),
                })
                form.reset()
                onSubmitted(operationId, false)
            } catch (error) {
                if (!sent) {
                    setErrorMessage(
                        error instanceof Error
                            ? error.message
                            : "无法读取私钥文件，请重新选择。",
                    )
                    return
                }
                const failure = jdVopAuthorizationError(error)
                form.setFieldValue("password", "")
                form.setFieldValue("privateKeyFile", null)
                if (failure.uncertain) {
                    form.reset()
                    onSubmitted(operationId, true)
                } else {
                    setErrorMessage(`${failure.message} 密码和私钥已清空。`)
                }
            } finally {
                privateKey = ""
                form.setFieldValue("password", "")
                form.setFieldValue("privateKeyFile", null)
                authorize.clearSecrets()
            }
        },
    })
    const submitting = useStore(form.store, (state) => state.isSubmitting)
    const dirty = useStore(form.store, (state) => !state.isDefaultValue)
    const busy = submitting || authorize.isPending
    const clearSecrets = authorize.clearSecrets
    useEffect(
        () => () => {
            form.reset()
            clearSecrets()
        },
        [form, clearSecrets],
    )
    const forceClose = () => {
        setDiscardOpen(false)
        setShowPassword(false)
        form.reset()
        clearSecrets()
        onClose()
    }
    const requestClose = () => {
        if (busy) return
        if (dirty) {
            setDiscardOpen(true)
            return
        }
        forceClose()
    }
    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open) requestClose()
                }}
            >
                <DialogContent
                    id={`${idPrefix}-dialog`}
                    size="md"
                    closeButtonId={`${idPrefix}-close`}
                    showCloseButton={!busy}
                >
                    <DialogHeader>
                        <DialogTitle>
                            {status.state === "missing"
                                ? "授权京东采购账号"
                                : "重新授权京东采购账号"}
                        </DialogTitle>
                        <DialogDescription>
                            使用京东企业采购账号办理授权。应用配置由服务器提供，授权后不会开启自动下单。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        id={`${idPrefix}-form`}
                        autoComplete="off"
                        className="space-y-6"
                        onSubmit={(event) => {
                            event.preventDefault()
                            event.stopPropagation()
                            if (!form.state.isSubmitting)
                                void form.handleSubmit()
                        }}
                    >
                        <fieldset disabled={busy} className="space-y-5">
                            <form.AppField name="username">
                                {(field) => (
                                    <field.TextField
                                        id={`${idPrefix}-username`}
                                        label="企业采购账号"
                                        autoComplete="off"
                                        required
                                        disabled={busy}
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="password">
                                {(field) => (
                                    <div className="relative">
                                        <field.TextField
                                            id={`${idPrefix}-password`}
                                            label="账号密码"
                                            type={
                                                showPassword
                                                    ? "text"
                                                    : "password"
                                            }
                                            autoComplete="off"
                                            required
                                            disabled={busy}
                                            inputClassName="pr-9"
                                            description="密码仅用于本次授权，提交完成或关闭窗口后清空。"
                                        />
                                        <Button
                                            id={`${idPrefix}-password-toggle`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label={
                                                showPassword
                                                    ? "隐藏账号密码"
                                                    : "临时显示账号密码"
                                            }
                                            aria-pressed={showPassword}
                                            disabled={busy}
                                            className="absolute top-6 right-1.5 text-muted-foreground hover:text-foreground"
                                            onClick={() =>
                                                setShowPassword((prev) => !prev)
                                            }
                                        >
                                            {showPassword ? (
                                                <EyeOffIcon
                                                    className="size-3.5"
                                                    aria-hidden="true"
                                                />
                                            ) : (
                                                <EyeIcon
                                                    className="size-3.5"
                                                    aria-hidden="true"
                                                />
                                            )}
                                        </Button>
                                    </div>
                                )}
                            </form.AppField>
                            <form.AppField name="redirectUri">
                                {(field) => (
                                    <field.TextField
                                        id={`${idPrefix}-redirect-uri`}
                                        label="应用回调地址"
                                        type="url"
                                        autoComplete="off"
                                        required
                                        disabled={busy}
                                        description="须与京东应用登记的回调地址完全一致，无需另外创建 ERP 回调页面。"
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="privateKeyFile">
                                {() => (
                                    <JdVopPrivateKeyField
                                        id={`${idPrefix}-private-key`}
                                        disabled={busy}
                                    />
                                )}
                            </form.AppField>
                            {recovering ? (
                                <form.AppField name="evidenceReference">
                                    {(field) => (
                                        <field.TextareaField
                                            id={`${idPrefix}-evidence`}
                                            label="原授权核实依据"
                                            required
                                            disabled={busy}
                                            description="原授权结果待核实。请先核实京东账号的授权情况，再填写核实结论或凭证编号，并使用原采购账号重新授权。"
                                        />
                                    )}
                                </form.AppField>
                            ) : null}
                        </fieldset>
                        {errorMessage ? (
                            <Alert variant="destructive">
                                <AlertDescription>
                                    {errorMessage}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        <DialogFooter>
                            <Button
                                id={`${idPrefix}-cancel`}
                                type="button"
                                variant="outline"
                                disabled={busy}
                                onClick={requestClose}
                            >
                                取消
                            </Button>
                            <LoadingButton
                                id={`${idPrefix}-submit`}
                                type="submit"
                                loading={busy}
                                disabled={busy}
                            >
                                {busy ? "正在提交授权…" : "提交授权"}
                            </LoadingButton>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix={`${idPrefix}-discard`}
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃尚未提交的授权资料？"
                description="关闭后本次填写的账号、密码与私钥选择将被清空。"
                confirmLabel="清空并关闭"
                cancelLabel="继续填写"
                onConfirm={forceClose}
            />
        </>
    )
}
