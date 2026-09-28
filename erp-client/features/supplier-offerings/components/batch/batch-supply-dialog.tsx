"use client"
import * as React from "react"
import { DiscardConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import type { FixedSku, SupplierOfferingView } from "../../types"
import {
    MODE_LABELS,
    newBatchRow,
    type BatchMode,
} from "../../lib/batch-supply"
import {
    downloadFailedRows,
    downloadSupplyTemplate,
} from "../../lib/batch-supply-import"
import { errorMessage } from "../../lib/offering-forms"
import { useBatchSupplyEditor } from "../../hooks/use-batch-supply-editor"
import { BatchCommonSettings } from "./batch-common-settings"
import { BatchSupplyTable } from "./batch-supply-table"

type Props = {
    mode: BatchMode
    skus?: readonly FixedSku[]
    offerings?: readonly SupplierOfferingView[]
    supplierId?: string
    onClose: () => void
}
export function BatchSupplyDialog(props: Props) {
    const profile = useAccountProfileQuery()
    if (!profile.data) return null
    return <BatchSupplyEditor {...props} userId={profile.data.userid} />
}
function BatchSupplyEditor({
    mode,
    skus = [],
    offerings = [],
    supplierId = "",
    onClose,
    userId,
}: Props & { userId: string }) {
    const editor = useBatchSupplyEditor({
        mode,
        skus,
        offerings,
        supplierId,
        userId,
        onClose,
    })
    const [reloadConfirm, setReloadConfirm] = React.useState(false)
    const [discard, setDiscard] = React.useState(false)
    const [templateBusy, setTemplateBusy] = React.useState(false)
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
                            {mode === "create"
                                ? "一个供应商，多个公司 SKU。填写公共设置后逐行核对，已有供给不会被覆盖。"
                                : mode === "revise"
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
                            const frozen = values.rows.some(
                                (row) =>
                                    row.status === "UNKNOWN" ||
                                    row.status === "SUCCEEDED",
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
                                            <div
                                                role="alert"
                                                className="rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950"
                                            >
                                                有 {unknown.length}{" "}
                                                行结果待确认，内容已锁定。关闭后可在当前浏览器标签页重新打开此操作恢复；不要清除浏览器会话数据。
                                            </div>
                                        )}
                                        <fieldset
                                            disabled={editor.busy}
                                            className="grid min-w-0 grid-cols-1 gap-4 md:grid-cols-2"
                                        >
                                            {mode === "create" && (
                                                <editor.form.AppField name="supplierId">
                                                    {(field) => (
                                                        <div className="space-y-2">
                                                            <label
                                                                htmlFor="batch-supply-supplier"
                                                                className="text-sm font-medium"
                                                            >
                                                                供应商 *
                                                            </label>
                                                            <SupplierSearchCombobox
                                                                id="batch-supply-supplier"
                                                                value={
                                                                    field.state
                                                                        .value ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) => {
                                                                    field.handleChange(
                                                                        value ??
                                                                            "",
                                                                    )
                                                                    editor.form.setFieldValue(
                                                                        "rows",
                                                                        values.rows.map(
                                                                            (
                                                                                row,
                                                                            ) => ({
                                                                                ...row,
                                                                                status: "DRAFT",
                                                                                message:
                                                                                    "",
                                                                            }),
                                                                        ),
                                                                    )
                                                                }}
                                                                onBlur={
                                                                    field.handleBlur
                                                                }
                                                                disabled={
                                                                    frozen
                                                                }
                                                                purpose="supplier-offering"
                                                                aria-label="批量供给供应商"
                                                                allowClear={
                                                                    false
                                                                }
                                                                className="w-full"
                                                            />
                                                        </div>
                                                    )}
                                                </editor.form.AppField>
                                            )}
                                            <editor.form.AppField name="changeReason">
                                                {(field) => (
                                                    <field.TextField
                                                        id="batch-supply-reason"
                                                        label="变更原因 *"
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
                                            {mode === "create" && (
                                                <>
                                                    <div className="min-w-52 flex-1">
                                                        <CompanySkuSearchCombobox
                                                            id="batch-supply-add-sku"
                                                            value={undefined}
                                                            onValueChange={() => {}}
                                                            onItemChange={(
                                                                item,
                                                            ) => {
                                                                if (item)
                                                                    editor.addRows(
                                                                        [
                                                                            newBatchRow(
                                                                                {
                                                                                    skuId: item.productId,
                                                                                    skuCode:
                                                                                        item.sku ??
                                                                                        "",
                                                                                    skuName:
                                                                                        item.name,
                                                                                    specification:
                                                                                        item.description ??
                                                                                        "",
                                                                                    baseUnit:
                                                                                        "",
                                                                                },
                                                                            ),
                                                                        ],
                                                                    )
                                                            }}
                                                            aria-label="添加公司 SKU"
                                                            placeholder="搜索并添加公司 SKU"
                                                            disabled={
                                                                editor.busy
                                                            }
                                                            className="w-full"
                                                        />
                                                    </div>
                                                    <Button
                                                        id="batch-supply-template"
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={
                                                            templateBusy ||
                                                            editor.busy
                                                        }
                                                        onClick={async () => {
                                                            setTemplateBusy(
                                                                true,
                                                            )
                                                            try {
                                                                await downloadSupplyTemplate()
                                                            } catch (error) {
                                                                editor.message(
                                                                    errorMessage(
                                                                        error,
                                                                        "模板下载失败",
                                                                    ),
                                                                )
                                                            } finally {
                                                                setTemplateBusy(
                                                                    false,
                                                                )
                                                            }
                                                        }}
                                                    >
                                                        下载供给模板
                                                    </Button>
                                                    <label
                                                        htmlFor="batch-supply-file"
                                                        className="text-xs text-muted-foreground"
                                                    >
                                                        导入文件
                                                        <input
                                                            id="batch-supply-file"
                                                            type="file"
                                                            accept=".xlsx,.csv"
                                                            disabled={
                                                                editor.busy
                                                            }
                                                            className="block max-w-52 text-xs"
                                                            onChange={(
                                                                event,
                                                            ) => {
                                                                const file =
                                                                    event.target
                                                                        .files?.[0]
                                                                event.target.value =
                                                                    ""
                                                                if (file)
                                                                    void editor.importFile(
                                                                        file,
                                                                    )
                                                            }}
                                                        />
                                                    </label>
                                                </>
                                            )}
                                            <span className="text-xs text-muted-foreground">
                                                共 {values.rows.length} 行 ·
                                                已勾选 {selected.length} 行
                                            </span>
                                        </div>
                                        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                                            <p>
                                                可从 Excel
                                                复制多格，点击对应单元格粘贴。订货编码须填写供应商使用的编码。
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
                                        {mode !== "create" &&
                                            invalid.length > 0 && (
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
                                                            setReloadConfirm(
                                                                true,
                                                            )
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
                                                            <Button
                                                                id="batch-supply-reload-confirm"
                                                                type="button"
                                                                size="sm"
                                                                onClick={() => {
                                                                    setReloadConfirm(
                                                                        false,
                                                                    )
                                                                    void editor.reloadFailed()
                                                                }}
                                                            >
                                                                确认重新读取
                                                            </Button>
                                                            <Button
                                                                id="batch-supply-reload-cancel"
                                                                type="button"
                                                                size="sm"
                                                                variant="ghost"
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
                                        <Button
                                            id="batch-supply-validate"
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
                                        </Button>
                                        {unknown.length > 0 && (
                                            <Button
                                                id="batch-supply-recover"
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
                                            </Button>
                                        )}
                                        <Button
                                            id="batch-supply-submit"
                                            type="submit"
                                            disabled={
                                                editor.busy || !selected.length
                                            }
                                        >
                                            {editor.busy
                                                ? "正在处理…"
                                                : `校验并提交 ${selected.length} 行`}
                                        </Button>
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
