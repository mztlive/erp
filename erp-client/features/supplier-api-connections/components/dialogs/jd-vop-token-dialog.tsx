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
import { isApiError } from "@/lib/api/errors"
import { useConfigureJdVopTokens } from "../../hooks/jd-vop-tokens"
import { jdVopAuthorizationError } from "../../lib/jd-vop-authorization"
import type { JdVopAuthorizationStatus } from "../../types/jd-vop-authorization"

const token = z
    .string()
    .min(1, "请输入令牌")
    .refine(
        (value) =>
            new TextEncoder().encode(value).byteLength <= 8192 &&
            !/[\s\p{Cc}]/u.test(value),
        "令牌不能包含空白或控制字符，且不能超过 8192 字节",
    )

export function JdVopTokenDialog({
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
    const save = useConfigureJdVopTokens(connectionId)
    const [errorMessage, setErrorMessage] = useState<string | null>(null)
    const [showAccessToken, setShowAccessToken] = useState(false)
    const [showRefreshToken, setShowRefreshToken] = useState(false)
    const [discardOpen, setDiscardOpen] = useState(false)
    const form = useAppForm({
        defaultValues: { accessToken: "", refreshToken: "", expiresAt: "" },
        validators: {
            onSubmit: z.object({
                accessToken: token,
                refreshToken: token,
                expiresAt: z
                    .string()
                    .refine(
                        (value) => new Date(value).getTime() > Date.now(),
                        "请填写本次令牌的实际到期时间，须晚于当前时间",
                    ),
            }),
        },
        onSubmit: async ({ value }) => {
            setErrorMessage(null)
            const operationId = crypto.randomUUID()
            try {
                await save.submit({
                    operation_id: operationId,
                    expected_version: status.version,
                    access_token: value.accessToken,
                    refresh_token: value.refreshToken,
                    expires_at: Math.floor(
                        new Date(value.expiresAt).getTime() / 1000,
                    ),
                })
                onSubmitted(operationId, false)
            } catch (error) {
                const failure = jdVopAuthorizationError(error)
                if (failure.uncertain) onSubmitted(operationId, true)
                else
                    setErrorMessage(
                        isApiError(error) && error.status === 409
                            ? "令牌记录已变化或授权正在处理。请关闭窗口，刷新状态后重新配置。"
                            : "令牌未保存，两项令牌已清空。请核对令牌、到期时间和京东授权权限后重新填写。",
                    )
            } finally {
                form.setFieldValue("accessToken", "")
                form.setFieldValue("refreshToken", "")
                save.clearSecrets()
            }
        },
    })
    const submitting = useStore(form.store, (state) => state.isSubmitting)
    const dirty = useStore(form.store, (state) => !state.isDefaultValue)
    const busy = submitting || save.isPending
    const clearSecrets = save.clearSecrets
    useEffect(
        () => () => {
            form.reset()
            clearSecrets()
        },
        [form, clearSecrets],
    )
    const forceClose = () => {
        setDiscardOpen(false)
        setShowAccessToken(false)
        setShowRefreshToken(false)
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
                    closeButtonId={`${idPrefix}-close`}
                    showCloseButton={!busy}
                    className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg"
                >
                    <DialogHeader>
                        <DialogTitle>配置京东令牌</DialogTitle>
                        <DialogDescription>
                            填写同次授权取得的两项令牌，须属于此连接的采购账号及服务端配置的京东应用。保存后接口调用使用这份配置，续期成功会同步更新。
                        </DialogDescription>
                    </DialogHeader>
                    <p className="text-xs text-muted-foreground">
                        请由 ERP
                        统一刷新令牌，避免另一套系统同时刷新导致令牌失效。已保存的令牌不会回显；更新时须重新填写两项令牌。
                    </p>
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
                            <form.AppField name="accessToken">
                                {(field) => (
                                    <div className="relative">
                                        <field.TextField
                                            id={`${idPrefix}-access-token`}
                                            label="Access token"
                                            type={
                                                showAccessToken
                                                    ? "text"
                                                    : "password"
                                            }
                                            autoComplete="new-password"
                                            required
                                            disabled={busy}
                                            inputClassName="pr-9"
                                        />
                                        <Button
                                            id={`${idPrefix}-access-token-toggle`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label={
                                                showAccessToken
                                                    ? "隐藏 Access token"
                                                    : "临时显示 Access token"
                                            }
                                            aria-pressed={showAccessToken}
                                            disabled={busy}
                                            className="absolute top-6 right-1.5 text-muted-foreground hover:text-foreground"
                                            onClick={() =>
                                                setShowAccessToken(
                                                    (prev) => !prev,
                                                )
                                            }
                                        >
                                            {showAccessToken ? (
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
                            <form.AppField name="refreshToken">
                                {(field) => (
                                    <div className="relative">
                                        <field.TextField
                                            id={`${idPrefix}-refresh-token`}
                                            label="Refresh token"
                                            type={
                                                showRefreshToken
                                                    ? "text"
                                                    : "password"
                                            }
                                            autoComplete="new-password"
                                            required
                                            disabled={busy}
                                            inputClassName="pr-9"
                                        />
                                        <Button
                                            id={`${idPrefix}-refresh-token-toggle`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label={
                                                showRefreshToken
                                                    ? "隐藏 Refresh token"
                                                    : "临时显示 Refresh token"
                                            }
                                            aria-pressed={showRefreshToken}
                                            disabled={busy}
                                            className="absolute top-6 right-1.5 text-muted-foreground hover:text-foreground"
                                            onClick={() =>
                                                setShowRefreshToken(
                                                    (prev) => !prev,
                                                )
                                            }
                                        >
                                            {showRefreshToken ? (
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
                            <form.AppField name="expiresAt">
                                {(field) => (
                                    <field.DateTimeField
                                        id={`${idPrefix}-expires-at`}
                                        label="Access token 到期时间（本地时间）"
                                        timeZone={
                                            Intl.DateTimeFormat().resolvedOptions()
                                                .timeZone
                                        }
                                        showTimeZone={false}
                                        required
                                        disabled={busy}
                                        description="填写京东本次授权返回的实际到期时间；不要填写 refresh token 的有效期，也不要按保存时间推算。"
                                    />
                                )}
                            </form.AppField>
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
                                {busy ? "正在保存…" : "保存令牌"}
                            </LoadingButton>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix={`${idPrefix}-discard`}
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃尚未保存的令牌配置？"
                description="关闭后本次填写的令牌与到期时间将被清空。"
                confirmLabel="清空并关闭"
                cancelLabel="继续填写"
                onConfirm={forceClose}
            />
        </>
    )
}
