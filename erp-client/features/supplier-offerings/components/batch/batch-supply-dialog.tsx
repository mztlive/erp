"use client"

import * as React from "react"
import { DiscardConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { SupplierOfferingView } from "../../types"
import { MODE_LABELS, type BatchMode } from "../../lib/batch-supply"
import { downloadFailedRows } from "../../lib/batch-supply-transfer"
import { useBatchSupplyEditor } from "../../hooks/use-batch-supply-editor"
import { BatchCommonSettings } from "./batch-common-settings"
import { BatchSupplyTable } from "./batch-supply-table"

type Props = {
    mode: BatchMode
    offerings?: readonly SupplierOfferingView[]
    onClose: () => void
}
export function BatchSupplyDialog(props: Props) {
    const profile = useAccountProfileQuery()
    if (!profile.data) return null
    return <BatchSupplyEditor {...props} userId={profile.data.userid} />
}
function BatchSupplyEditor({
    mode,
    offerings = [],
    onClose,
    userId,
}: Props & { userId: string }) {
    const editor = useBatchSupplyEditor({
        mode,
        offerings,
        userId,
        onClose,
    })
    const [reloadConfirm, setReloadConfirm] = React.useState(false)
    const [discard, setDiscard] = React.useState(false)
    const requestClose = () => {
        if (editor.busy) return
        const values = editor.form.state.values
        if (values.rows.some((row) => row.status === "UNKNOWN")) {
            sessionStorage.setItem(editor.storageKey, JSON.stringify(values))
            onClose()
            return
        }
        if (
            editor.form.state.isDirty &&
            values.rows.some((row) => row.status !== "SUCCEEDED")
        )
            setDiscard(true)
        else onClose()
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
                    closeButtonId="batch-supply-close-icon"
                    showCloseButton={!editor.busy}
                    className="flex max-h-[92dvh] w-[calc(100%-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[1280px]"
                >
                    <DialogHeader className="shrink-0 border-b px-6 py-5">
                        <DialogTitle>{MODE_LABELS[mode]}</DialogTitle>
                        <DialogDescription>
                            {mode === "revise"
                                ? "按行追加价格与商业条款版本，保持当前可供数量。"
                                : "只调整可供状态和数量，保留现有价格与商业条款。"}
                        </DialogDescription>
                    </DialogHeader>
                    <editor.form.Subscribe selector={(state) => state.values}>
                        {(values) => {
                            const selected = values.rows.filter(
                                (row) =>
                                    row.selected && row.status !== "SUCCEEDED",
                            )
                            const unknown = values.rows.filter(
                                (row) => row.status === "UNKNOWN",
                            )
                            const invalid = values.rows.filter(
                                (row) =>
                                    row.status === "INVALID" ||
                                    row.status === "FAILED",
                            )
                            return (
                                <form
                                    className="flex min-h-0 flex-1 flex-col"
                                    onSubmit={(event) => {
                                        event.preventDefault()
                                        event.stopPropagation()
                                        void editor.form.handleSubmit()
                                    }}
                                >
                                    <div className="min-h-0 space-y-5 overflow-y-auto px-6 py-5">
                                        {values.message && (
                                            <p
                                                role="status"
                                                className="rounded-lg border bg-muted/30 p-3 text-sm"
                                            >
                                                {values.message}
                                            </p>
                                        )}
                                        {unknown.length > 0 && (
                                            <Alert
                                                variant="warning"
                                                role="alert"
                                            >
                                                <AlertDescription>
                                                    有 {unknown.length}{" "}
                                                    行结果待确认，内容已锁定。关闭后可在当前浏览器标签页重新打开此操作恢复；不要清除浏览器会话数据。
                                                </AlertDescription>
                                            </Alert>
                                        )}
                                        <fieldset
                                            disabled={editor.busy}
                                            className="grid min-w-0 grid-cols-1 gap-4 md:grid-cols-2"
                                        >
                                            <editor.form.AppField name="changeReason">
                                                {(field) => (
                                                    <field.TextField
                                                        id="batch-supply-reason"
                                                        label="变更原因"
                                                        required
                                                        disabled={
                                                            unknown.length > 0
                                                        }
                                                    />
                                                )}
                                            </editor.form.AppField>
                                        </fieldset>
                                        <fieldset disabled={editor.busy}>
                                            <BatchCommonSettings
                                                editor={editor}
                                            />
                                        </fieldset>
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="text-xs text-muted-foreground">
                                                共 {values.rows.length} 行 ·
                                                已勾选 {selected.length} 行
                                            </span>
                                        </div>
                                        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                                            <p>
                                                可从 Excel
                                                复制多格，点击对应的价格或数量单元格粘贴。
                                            </p>
                                            {invalid.length > 0 && (
                                                <Button
                                                    id="batch-supply-deselect-invalid"
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    disabled={editor.busy}
                                                    onClick={
                                                        editor.deselectInvalid
                                                    }
                                                >
                                                    取消勾选异常行（
                                                    {invalid.length}）
                                                </Button>
                                            )}
                                        </div>
                                        {invalid.length > 0 && (
                                            <div className="rounded-lg border p-3 text-sm">
                                                <p>
                                                    版本冲突时，先重新读取异常行的最新内容，再填写本次修改。已完成和待确认的行保持锁定。
                                                </p>
                                                <Button
                                                    id="batch-supply-reload-failed"
                                                    type="button"
                                                    variant="outline"
                                                    size="sm"
                                                    disabled={editor.busy}
                                                    onClick={() =>
                                                        setReloadConfirm(true)
                                                    }
                                                >
                                                    重新读取异常行…
                                                </Button>
                                                {reloadConfirm && (
                                                    <div
                                                        className="mt-2 flex flex-wrap items-center gap-2"
                                                        role="alert"
                                                    >
                                                        <span>
                                                            将替换异常行的当前输入，请确认。
                                                        </span>
                                                        <LoadingButton
                                                            id="batch-supply-reload-confirm"
                                                            loading={
                                                                editor.reloadPending
                                                            }
                                                            type="button"
                                                            size="sm"
                                                            disabled={
                                                                editor.busy
                                                            }
                                                            onClick={async () => {
                                                                try {
                                                                    await editor.reloadFailed()
                                                                } finally {
                                                                    setReloadConfirm(
                                                                        false,
                                                                    )
                                                                }
                                                            }}
                                                        >
                                                            确认重新读取
                                                        </LoadingButton>
                                                        <Button
                                                            id="batch-supply-reload-cancel"
                                                            type="button"
                                                            size="sm"
                                                            variant="ghost"
                                                            disabled={
                                                                editor.busy
                                                            }
                                                            onClick={() =>
                                                                setReloadConfirm(
                                                                    false,
                                                                )
                                                            }
                                                        >
                                                            取消
                                                        </Button>
                                                    </div>
                                                )}
                                            </div>
                                        )}
                                        <BatchSupplyTable
                                            editor={editor}
                                            rows={values.rows}
                                        />
                                    </div>
                                    <DialogFooter className="shrink-0 flex-wrap border-t bg-background px-6 py-4">
                                        {values.rows.some(
                                            (row) =>
                                                row.status === "INVALID" ||
                                                row.status === "FAILED" ||
                                                row.status === "UNKNOWN",
                                        ) && (
                                            <Button
                                                id="batch-supply-export-errors"
                                                type="button"
                                                variant="ghost"
                                                disabled={editor.busy}
                                                onClick={() =>
                                                    downloadFailedRows(
                                                        values.rows,
                                                    )
                                                }
                                            >
                                                导出未完成明细
                                            </Button>
                                        )}
                                        <Button
                                            id="batch-supply-cancel"
                                            type="button"
                                            variant="outline"
                                            disabled={editor.busy}
                                            onClick={requestClose}
                                        >
                                            {unknown.length
                                                ? "关闭并保留记录"
                                                : "取消"}
                                        </Button>
                                        <LoadingButton
                                            id="batch-supply-validate"
                                            loading={
                                                editor.mutation.isPending &&
                                                !!editor.mutation.variables
                                                    ?.validateOnly
                                            }
                                            type="button"
                                            variant="outline"
                                            disabled={
                                                editor.busy || !selected.length
                                            }
                                            onClick={() =>
                                                void editor.run(true)
                                            }
                                        >
                                            校验勾选行
                                        </LoadingButton>
                                        {unknown.length > 0 && (
                                            <LoadingButton
                                                id="batch-supply-recover"
                                                loading={
                                                    editor.mutation.isPending &&
                                                    !!editor.mutation.variables
                                                        ?.recoveryOnly
                                                }
                                                type="button"
                                                disabled={editor.busy}
                                                onClick={() => {
                                                    editor.form.setFieldValue(
                                                        "rows",
                                                        values.rows.map(
                                                            (row) =>
                                                                row.status ===
                                                                "UNKNOWN"
                                                                    ? {
                                                                          ...row,
                                                                          selected: true,
                                                                      }
                                                                    : row,
                                                        ),
                                                    )
                                                    void editor.run(false, true)
                                                }}
                                            >
                                                确认待定结果（{unknown.length}）
                                            </LoadingButton>
                                        )}
                                        <LoadingButton
                                            id="batch-supply-submit"
                                            loading={
                                                editor.mutation.isPending &&
                                                !editor.mutation.variables
                                                    ?.validateOnly &&
                                                !editor.mutation.variables
                                                    ?.recoveryOnly
                                            }
                                            type="submit"
                                            disabled={
                                                editor.busy || !selected.length
                                            }
                                        >
                                            {editor.busy
                                                ? "正在处理…"
                                                : `校验并提交 ${selected.length} 行`}
                                        </LoadingButton>
                                    </DialogFooter>
                                </form>
                            )
                        }}
                    </editor.form.Subscribe>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix="batch-supply-discard"
                open={discard}
                onOpenChange={setDiscard}
                onConfirm={() => {
                    sessionStorage.removeItem(editor.storageKey)
                    setDiscard(false)
                    onClose()
                }}
            />
        </>
    )
}
