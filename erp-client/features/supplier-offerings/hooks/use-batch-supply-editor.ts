"use client"
import * as React from "react"
import { useAppForm } from "@/components/form"
import { isApiError } from "@/lib/api/errors"
import { toast } from "@/components/ui/toast"
import type { FixedSku, SupplierOfferingView } from "../types"
import {
    batchDefaults,
    offeringBatchRow,
    BATCH_LIMIT,
    lockedRow,
    MODE_LABELS,
    rowErrors,
    rowInput,
    type BatchMode,
    type BatchRow,
    type BatchValues,
    type TextFieldKey,
} from "../lib/batch-supply"
import { errorMessage, idempotencyKey } from "../lib/offering-forms"
import { parseDelimited } from "../lib/batch-supply-import"
import {
    batchValidationMessage,
    SupplyBatchValidationError,
} from "../lib/batch-supply-error"
import {
    useReloadSupplyRowsMutation,
    useSupplyBatchMutation,
    useSupplyFileMutation,
} from "./use-supply-batch"

/** 只保存已经发起提交的快照，按用户和操作类型隔离恢复。 */
function restore(key: string, defaults: BatchValues): BatchValues {
    try {
        const text = sessionStorage.getItem(key)
        if (!text) return defaults
        const saved = JSON.parse(text) as BatchValues
        if (
            !Array.isArray(saved.rows) ||
            saved.rows.length > BATCH_LIMIT ||
            !saved.rows.every(
                (row) =>
                    typeof row.rowId === "string" &&
                    typeof row.skuId === "string" &&
                    typeof row.key === "string",
            )
        )
            return defaults
        return {
            ...defaults,
            ...saved,
            message:
                "已恢复上次提交记录。待确认的行须保持原内容重试；已完成的行不会重复提交。",
        }
    } catch {
        return defaults
    }
}
export function useBatchSupplyEditor({
    mode,
    skus,
    offerings,
    supplierId,
    userId,
    onClose,
}: {
    mode: BatchMode
    skus: readonly FixedSku[]
    offerings: readonly SupplierOfferingView[]
    supplierId: string
    userId: string
    onClose: () => void
}) {
    const mutation = useSupplyBatchMutation(mode)
    const fileMutation = useSupplyFileMutation()
    const reloadMutation = useReloadSupplyRowsMutation()
    const storageKey = `supply-batch-v1:${userId}:${mode}`
    const [defaults] = React.useState(() =>
        restore(storageKey, batchDefaults(mode, skus, offerings, supplierId)),
    )
    const form = useAppForm({
        defaultValues: defaults,
        onSubmit: async () => {
            await run(false)
        },
    })
    const busy =
        mutation.isPending || fileMutation.isPending || reloadMutation.isPending
    const message = (text: string) => form.setFieldValue("message", text)
    const changeRows = (rows: BatchRow[]) => form.setFieldValue("rows", rows)
    const changed = (row: BatchRow): BatchRow => ({
        ...row,
        status: "DRAFT",
        message: "",
        frozen: null,
        key: idempotencyKey("batch-supply"),
    })
    function editRow(rowId: string, patch: Partial<BatchRow>) {
        changeRows(
            form.state.values.rows.map((row) =>
                row.rowId === rowId && !lockedRow(row)
                    ? changed({ ...row, ...patch })
                    : row,
            ),
        )
    }
    function editCell(rowId: string, key: TextFieldKey, value: string) {
        const row = form.state.values.rows.find((item) => item.rowId === rowId)
        const patch: Partial<BatchRow> = { [key]: value }
        if (key === "dropshipPrice" && row?.samePrice) patch.bulkPrice = value
        if (key === "validTo") patch.validityMode = value ? "dated" : "ongoing"
        if (key === "availableQuantity")
            patch.quantityMode = value ? "provided" : "unknown"
        editRow(rowId, patch)
    }
    function selectRow(rowId: string, selected: boolean) {
        changeRows(
            form.state.values.rows.map((row) =>
                row.rowId === rowId && row.status !== "SUCCEEDED"
                    ? { ...row, selected }
                    : row,
            ),
        )
    }
    function addRows(rows: BatchRow[]) {
        const current = form.state.values.rows
        if (current.length + rows.length > BATCH_LIMIT) {
            message(`每批最多 ${BATCH_LIMIT} 行，请分批添加`)
            return
        }
        if (
            rows.some((row) =>
                current.some((existing) => existing.rowId === row.rowId),
            )
        ) {
            message("所选 SKU 已在表格中，请直接编辑已有行")
            return
        }
        changeRows([...current, ...rows])
        message("")
    }
    function applyCommon(overwrite: boolean) {
        const { common, rows, applyQuantity, applyAvailabilityStatus } =
            form.state.values
        const fields: TextFieldKey[] =
            mode === "availability"
                ? ["availableQuantity"]
                : [
                      "dropshipPrice",
                      "bulkPrice",
                      "inputTaxPercentage",
                      "minimumQuantity",
                      "supplyRegionText",
                      "validFrom",
                      "validTo",
                      "dropshipExpress",
                      "freightAmount",
                      "serviceFeeAmount",
                  ]
        changeRows(
            rows.map((row) => {
                if (!row.selected || lockedRow(row)) return row
                const next = { ...row }
                for (const key of fields)
                    if (
                        (key !== "availableQuantity" || applyQuantity) &&
                        common[key].trim() &&
                        (overwrite || !row[key].trim())
                    )
                        next[key] = common[key]
                if (mode === "availability" && overwrite) {
                    if (applyAvailabilityStatus)
                        next.availabilityStatus = common.availabilityStatus
                    if (applyQuantity) {
                        next.quantityMode = common.quantityMode
                        if (common.quantityMode === "unknown")
                            next.availableQuantity = ""
                    }
                }
                if (next.availableQuantity) next.quantityMode = "provided"
                if (next.validTo) next.validityMode = "dated"
                if (next.samePrice) next.bulkPrice = next.dropshipPrice
                return changed(next)
            }),
        )
        message(
            overwrite
                ? "已将公共设置覆盖到勾选的可编辑行，请校验后提交。"
                : "已补齐勾选行的空白字段，已有值保留。",
        )
    }
    function paste(
        rowId: string,
        start: TextFieldKey,
        text: string,
        columns: TextFieldKey[],
    ) {
        try {
            const matrix = parseDelimited(text)
            const rows = [...form.state.values.rows]
            const rowIndex = rows.findIndex((row) => row.rowId === rowId),
                columnIndex = columns.indexOf(start)
            if (
                matrix.length + rowIndex > rows.length ||
                matrix.some((row) => row.length + columnIndex > columns.length)
            )
                throw new SupplyBatchValidationError(
                    "粘贴范围超出表格，请先添加足够的 SKU，并核对列顺序",
                )
            if (matrix.some((_, i) => lockedRow(rows[rowIndex + i])))
                throw new SupplyBatchValidationError(
                    "粘贴范围包含已完成或待确认的行，请调整范围",
                )
            matrix.forEach((cells, i) => {
                const row = { ...rows[rowIndex + i] }
                cells.forEach((value, j) => {
                    row[columns[columnIndex + j]] = value.trim()
                })
                if (row.samePrice) row.bulkPrice = row.dropshipPrice
                if (columns.includes("availableQuantity"))
                    row.quantityMode = row.availableQuantity
                        ? "provided"
                        : "unknown"
                rows[rowIndex + i] = changed({
                    ...row,
                    source: mode === "create" ? "EXCEL" : row.source,
                })
            })
            changeRows(rows)
            message(`已粘贴 ${matrix.length} 行，请核对后校验。`)
        } catch (error) {
            message(batchValidationMessage(error, "粘贴失败"))
        }
    }
    async function importFile(file: File) {
        try {
            addRows(await fileMutation.mutateAsync(file))
        } catch (error) {
            message(batchValidationMessage(error, "文件导入失败"))
        }
    }
    async function reloadFailed() {
        const rows = form.state.values.rows
        const targets = rows.filter(
            (row) => row.status === "FAILED" || row.status === "INVALID",
        )
        try {
            const items = await reloadMutation.mutateAsync(
                targets.map((row) => row.skuId),
            )
            changeRows(
                rows.map((row) => {
                    if (!targets.includes(row)) return row
                    const current = items.find(
                        (item) => item.id === row.offeringId,
                    )
                    return current
                        ? {
                              ...offeringBatchRow(current),
                              selected: row.selected,
                          }
                        : {
                              ...row,
                              message:
                                  "当前供给不可读取，请检查数据范围或联系维护人",
                          }
                }),
            )
            message("异常行已重新读取。请核对最新内容，再填写本次修改并校验。")
        } catch (error) {
            message(errorMessage(error, "读取最新供给失败，原输入已保留"))
        }
    }
    async function run(validateOnly: boolean, recoveryOnly = false) {
        const values = form.state.values
        const selected = values.rows.filter(
            (row) =>
                row.selected &&
                row.status !== "SUCCEEDED" &&
                (!recoveryOnly || row.status === "UNKNOWN"),
        )
        if (!selected.length) {
            message("请勾选需要处理的行")
            return
        }
        const errors = new Map(
            selected.map((row) => [row.rowId, rowErrors(row, values, mode)]),
        )
        if ([...errors.values()].some(Boolean)) {
            changeRows(
                values.rows.map((row) =>
                    errors.get(row.rowId)
                        ? {
                              ...row,
                              status: "INVALID",
                              message: errors.get(row.rowId)!,
                          }
                        : row,
                ),
            )
            message("请修正标红行，或明确取消勾选后再提交。")
            return
        }
        const inputs = selected.map((row) => ({
            row_id: row.rowId,
            input: rowInput(row, values, mode),
        }))
        const sending = new Map(inputs.map((row) => [row.row_id, row.input]))
        let snapshot = {
            ...values,
            rows: values.rows.map((row) =>
                sending.has(row.rowId) && !validateOnly
                    ? {
                          ...row,
                          status: "UNKNOWN" as const,
                          frozen: sending.get(row.rowId)!,
                      }
                    : row,
            ),
        }
        if (!validateOnly) {
            try {
                sessionStorage.setItem(storageKey, JSON.stringify(snapshot))
            } catch {
                message("浏览器无法保存本次提交记录，请释放会话存储后重试。")
                return
            }
            changeRows(snapshot.rows)
        }
        try {
            const result = await mutation.mutateAsync({
                rows: inputs,
                validateOnly,
                recoveryOnly,
            })
            snapshot = {
                ...snapshot,
                rows: snapshot.rows.map((row) => {
                    const response = result.rows.find(
                        (item) => item.row_id === row.rowId,
                    )
                    if (!response) return row
                    const originallyUnknown =
                        values.rows.find((item) => item.rowId === row.rowId)
                            ?.status === "UNKNOWN"
                    const status =
                        originallyUnknown && response.status !== "SUCCEEDED"
                            ? "UNKNOWN"
                            : response.status
                    return {
                        ...row,
                        status,
                        message: response.message ?? "",
                        frozen:
                            status === "UNKNOWN" || status === "SUCCEEDED"
                                ? sending.get(row.rowId)!
                                : null,
                    }
                }),
            }
            changeRows(snapshot.rows)
            if (!validateOnly || sessionStorage.getItem(storageKey))
                sessionStorage.setItem(storageKey, JSON.stringify(snapshot))
            const completed = snapshot.rows.filter(
                (row) => row.status === "SUCCEEDED",
            ).length
            if (
                !validateOnly &&
                snapshot.rows.every(
                    (row) =>
                        row.status !== "UNKNOWN" &&
                        (!row.selected || row.status === "SUCCEEDED"),
                )
            ) {
                sessionStorage.removeItem(storageKey)
                toast.add({
                    title: `${MODE_LABELS[mode]}完成`,
                    description: `成功 ${completed} 行；未勾选的行未提交。`,
                    type: "success",
                })
                onClose()
            } else
                message(
                    validateOnly
                        ? "校验结果已更新。修改字段后需要重新校验；提交时仍会检查最新版本。"
                        : `已完成 ${completed} 行。请处理剩余行，成功记录已锁定。`,
                )
        } catch (error) {
            if (
                !validateOnly &&
                isApiError(error) &&
                error.status != null &&
                error.status >= 400 &&
                error.status < 500 &&
                error.status !== 408
            ) {
                snapshot = {
                    ...snapshot,
                    rows: snapshot.rows.map((row) =>
                        sending.has(row.rowId) &&
                        values.rows.find((item) => item.rowId === row.rowId)
                            ?.status !== "UNKNOWN"
                            ? {
                                  ...row,
                                  status: "INVALID",
                                  frozen: null,
                                  message: errorMessage(
                                      error,
                                      "本批未提交，请检查输入",
                                  ),
                              }
                            : row,
                    ),
                }
                changeRows(snapshot.rows)
                sessionStorage.setItem(storageKey, JSON.stringify(snapshot))
                message(errorMessage(error, "本批未提交，请检查输入"))
                return
            }
            message(
                validateOnly
                    ? errorMessage(error, "校验失败，请重试")
                    : "连接中断，部分行可能已经保存。请点击「确认待定结果」，系统会恢复已有结果并完成剩余行。",
            )
        }
    }
    return {
        form,
        mode,
        busy,
        storageKey,
        mutation,
        reloadPending: reloadMutation.isPending,
        message,
        editRow,
        editCell,
        selectRow,
        addRows,
        applyCommon,
        paste,
        importFile,
        reloadFailed,
        run,
        samePrice: () =>
            changeRows(
                form.state.values.rows.map((row) =>
                    row.selected && !lockedRow(row)
                        ? changed({
                              ...row,
                              samePrice: true,
                              bulkPrice: row.dropshipPrice,
                          })
                        : row,
                ),
            ),
        deselectInvalid: () =>
            changeRows(
                form.state.values.rows.map((row) =>
                    row.status === "INVALID" || row.status === "FAILED"
                        ? { ...row, selected: false }
                        : row,
                ),
            ),
    }
}
export type BatchEditor = ReturnType<typeof useBatchSupplyEditor>
