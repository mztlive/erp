"use client"

import { useEffect, useRef, useState, type ClipboardEvent } from "react"
import Link from "next/link"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
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
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { getErrorMessage } from "@/lib/api/errors"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { portalApplication, portalCatalog, portalOfferings } from "../api"
import { portalKeys, usePortalBatch } from "../hooks/queries"
import { commandKey, offeringTerms } from "../lib/presentation"
import type {
    PortalBatchMode,
    PortalBatchPhase,
    PortalApplication,
    PortalBatchRow,
    PortalCatalogSku,
    PortalOffering,
    PortalPage,
} from "../types"
import {
    applyBatchResult,
    batchColumns,
    batchFields,
    batchFormSchema,
    batchModeLabels,
    batchProcessingCount,
    batchStatusLabels,
    editBatchValues,
    hasLegacyBatchReport,
    isBatchRowLocked,
    mainBatchColumns,
    markBatchUnknown,
    newBatchRow,
    PORTAL_BATCH_LIMIT,
    prepareBatchRows,
    validateBatchRows,
    type BatchColumn,
    type BatchEditorRow,
    type BatchField,
    type BatchTarget,
    type BatchValues,
} from "../lib/batch-model"
import {
    downloadPortalBatchErrors,
    downloadPortalBatchTemplate,
    importPortalText,
    pastePortalCells,
    readPortalBatchFile,
} from "../lib/batch-import"
import { usePortalProfile } from "./portal-session"
import { PortalBatchDraftEditor } from "./batch-draft-editor"

type StoredBatch = { version: 1; mode: PortalBatchMode; rows: BatchEditorRow[] }
function restoreRows(
    storageKey: string,
    mode: PortalBatchMode,
): BatchEditorRow[] {
    const raw = sessionStorage.getItem(storageKey)
    if (!raw) return []
    const saved = JSON.parse(raw) as StoredBatch
    if (
        saved.version !== 1 ||
        saved.mode !== mode ||
        !Array.isArray(saved.rows) ||
        saved.rows.length > PORTAL_BATCH_LIMIT ||
        new Set(saved.rows.map((row) => row.rowId)).size !==
            saved.rows.length ||
        !saved.rows.every(
            (row) =>
                typeof row.rowId === "string" &&
                typeof row.key === "string" &&
                (typeof row.reportedAt === "string" ||
                    (typeof row.reportedAt === "number" &&
                        Number.isSafeInteger(row.reportedAt))) &&
                typeof row.message === "string" &&
                row.fieldErrors &&
                typeof row.fieldErrors === "object" &&
                row.values &&
                batchFields.every(
                    (field) => typeof row.values[field] === "string",
                ) &&
                [
                    "EDITING",
                    "READY",
                    "INVALID",
                    "SUCCEEDED",
                    "FAILED",
                    "UNKNOWN",
                    "PREPARED",
                ].includes(row.status) &&
                (row.status !== "UNKNOWN" ||
                    (row.command &&
                        typeof row.command.row_id === "string" &&
                        typeof row.command.idempotency_key === "string" &&
                        typeof row.command.input === "object")),
        )
    )
        throw new Error(
            "上次提交记录无法读取，请先联系采购核对原申请或可供结果",
        )
    return saved.rows.map((row) =>
        row.status === "UNKNOWN" && hasLegacyBatchReport(row.command, mode)
            ? {
                  ...row,
                  message:
                      "原报送时间需要重新核对，请联系采购确认原操作结果；保留原内容及记录，勿重复提交",
              }
            : row,
    )
}
const uniqueCommands = (rows: BatchEditorRow[]) => [
    ...new Map(
        rows.flatMap((row) =>
            row.command ? [[row.command.row_id, row.command] as const] : [],
        ),
    ).values(),
]

/** 四类批量分别预检与提交；新品以商品组为单一处理单元。 */
export function PortalBatchDialog({
    mode,
    onClose,
}: {
    mode: PortalBatchMode
    onClose: () => void
}) {
    const profile = usePortalProfile()
    const client = useQueryClient()
    const storageKey = `supplier-portal-batch-v1:${profile?.account_id ?? "signed-out"}:${profile?.supplier_id ?? "unbound"}:${mode}`
    const prefix = `supplier-portal-batch-${mode.replaceAll("_", "-")}`
    const initialized = useRef<string | null>(null)
    const running = useRef(false)
    const command = usePortalBatch(mode)
    const [editingDraft, setEditingDraft] = useState<string | null>(null)
    const [reviewedTargets, setReviewedTargets] = useState<
        Record<string, { target: BatchTarget; key: string }>
    >({})
    const imported = useMutation({
        mutationFn: (source: File | string) =>
            typeof source === "string"
                ? Promise.resolve(importPortalText(source, mode))
                : readPortalBatchFile(source, mode),
        retry: false,
    })
    const template = useMutation({
        mutationFn: () => downloadPortalBatchTemplate(mode),
        retry: false,
    })
    const form = useAppForm({
        defaultValues: {
            paste: "",
            rows: [] as BatchEditorRow[],
            notice: "",
            hydrated: false,
            recoveryBlocked: false,
        },
        validators: { onSubmit: batchFormSchema },
        onSubmit: async () => {
            await run(false)
        },
    })
    useEffect(() => {
        if (initialized.current === storageKey) return
        initialized.current = storageKey
        try {
            const rows = restoreRows(storageKey, mode)
            form.setFieldValue("rows", rows)
            if (rows.length)
                form.setFieldValue(
                    "notice",
                    "已恢复本账号上次记录。已完成资料保持锁定；待确认资料须先确认原结果。",
                )
        } catch (cause) {
            form.setFieldValue("notice", getErrorMessage(cause))
            form.setFieldValue("recoveryBlocked", true)
        }
        form.setFieldValue("hydrated", true)
    }, [form, mode, storageKey])

    async function lookup(
        values: BatchValues,
        catalog: boolean,
    ): Promise<BatchTarget | undefined> {
        const q = values.skuNo.trim()
        const matches: BatchTarget[] = []
        for (let page = 1; ; page++) {
            const query = { q, page, page_size: 100 }
            const result: PortalPage<PortalCatalogSku | PortalOffering> =
                catalog
                    ? await client.fetchQuery({
                          queryKey: [...portalKeys.all, "batch-catalog", query],
                          queryFn: () => portalCatalog(query),
                          staleTime: 0,
                      })
                    : await client.fetchQuery({
                          queryKey: portalKeys.offerings(query),
                          queryFn: () => portalOfferings(query),
                          staleTime: 0,
                      })
            matches.push(
                ...result.items.filter(
                    (item) =>
                        item.sku_no === q &&
                        (catalog ||
                            ("supplier_sku_code" in item &&
                                item.supplier_sku_code ===
                                    values.orderingCode.trim())),
                ),
            )
            if (
                matches.length > 1 ||
                !result.items.length ||
                page * 100 >= result.total
            )
                break
        }
        return matches.length === 1 ? matches[0] : undefined
    }
    const prepared = useMutation({
        mutationFn: async (rows: BatchEditorRow[]) => {
            const checked = validateBatchRows(rows, mode)
            const targets = new Map<string, BatchTarget>()
            if (
                mode !== "new_product" &&
                !checked.some((row) => row.status === "INVALID")
            ) {
                const pending = checked.filter(
                    (row) => !isBatchRowLocked(row) && !row.command,
                )
                // 单批最多100行，限制匹配并发，避免逐键输入触发查询。
                for (let offset = 0; offset < pending.length; offset += 6)
                    await Promise.all(
                        pending.slice(offset, offset + 6).map(async (row) => {
                            const target = await lookup(
                                row.values,
                                mode === "quote",
                            )
                            if (target) targets.set(row.rowId, target)
                        }),
                    )
            }
            return prepareBatchRows(checked, mode, targets)
        },
        retry: false,
    })
    const reloaded = useMutation({
        mutationFn: (row: BatchEditorRow) =>
            lookup(row.values, mode === "quote"),
        retry: false,
    })
    const busy =
        command.isPending ||
        imported.isPending ||
        prepared.isPending ||
        reloaded.isPending
    const readOnly = profile?.role !== "maintainer"
    function persist(rows: BatchEditorRow[]) {
        sessionStorage.setItem(
            storageKey,
            JSON.stringify({ version: 1, mode, rows } satisfies StoredBatch),
        )
    }
    function changeRows(rows: BatchEditorRow[]) {
        form.setFieldValue("rows", rows)
        try {
            persist(rows)
        } catch {
            form.setFieldValue(
                "notice",
                "当前浏览器无法保存恢复记录。请检查会话存储后重试；提交前须成功保存记录。",
            )
        }
    }
    function changeCell(rowId: string, key: BatchField, value: string) {
        changeRows(
            editBatchValues(
                form.state.values.rows,
                new Map([[rowId, { [key]: value }]]),
                mode,
            ),
        )
        form.setFieldValue("notice", "")
    }
    function pasteCells(
        event: ClipboardEvent<HTMLInputElement>,
        rowId: string,
        key: BatchField,
    ) {
        const text = event.clipboardData.getData("text/plain")
        if (!/[\t\r\n]/.test(text)) return
        event.preventDefault()
        try {
            changeRows(
                pastePortalCells(
                    form.state.values.rows,
                    rowId,
                    key,
                    text,
                    mode,
                ),
            )
            form.setFieldValue("notice", "已按模板列顺序粘贴，尚未提交。")
        } catch (cause) {
            form.setFieldValue("notice", getErrorMessage(cause))
        }
    }
    async function importRows(source: File | string) {
        try {
            const rows = await imported.mutateAsync(source)
            const existing = form.state.values.rows
            if (existing.length + rows.length > PORTAL_BATCH_LIMIT)
                throw new Error("加入后超出100行，请先完成当前批次或拆分表格")
            const incomingGroups = new Set(
                rows.map((row) => row.values.groupCode.trim()),
            )
            const affected = new Map(
                existing
                    .filter(
                        (row) =>
                            mode === "new_product" &&
                            incomingGroups.has(row.values.groupCode.trim()) &&
                            !isBatchRowLocked(row),
                    )
                    .map((row) => [row.rowId, {}]),
            )
            changeRows([...editBatchValues(existing, affected, mode), ...rows])
            form.setFieldValue("paste", "")
            form.setFieldValue(
                "notice",
                mode === "new_product"
                    ? `已加入${rows.length}条数据；请校验后按商品组保存草稿。`
                    : `已加入${rows.length}条数据；请校验后提交。`,
            )
        } catch (cause) {
            form.setFieldValue(
                "notice",
                getErrorMessage(cause, "无法读取表格，请检查文件后重试"),
            )
        }
    }
    async function reloadRow(row: BatchEditorRow) {
        try {
            const target = await reloaded.mutateAsync(row)
            if (!target)
                throw new Error(
                    "未精确匹配当前目标，请核对SKU编号和自己的订货编码",
                )
            setReviewedTargets((value) => ({
                ...value,
                [row.rowId]: { target, key: row.key },
            }))
            const summary =
                "source_type" in target
                    ? `当前供给：${target.name ?? target.sku_name ?? "商品规格"}；代发含税价${offeringTerms(target).dropship_supply_price_gross}，集采含税价${offeringTerms(target).bulk_supply_price_gross}；可供数量${target.available_quantity ?? "未提供"}。原输入继续保留。`
                    : `当前规格：${target.name} / ${target.specification} / ${target.unit_name}。原报价继续保留，请核对是否仍适用。`
            changeRows(
                form.state.values.rows.map((item) =>
                    item.rowId === row.rowId
                        ? { ...item, message: summary }
                        : item,
                ),
            )
        } catch (cause) {
            form.setFieldValue("notice", getErrorMessage(cause))
        }
    }
    async function run(
        validateOnly: boolean,
        recoveryOnly = false,
        phase: PortalBatchPhase = "prepare",
    ) {
        if (
            running.current ||
            busy ||
            readOnly ||
            !form.state.values.hydrated ||
            form.state.values.recoveryBlocked
        )
            return
        running.current = true
        let writeStarted = false
        let commands: PortalBatchRow[] = []
        try {
            const current = form.state.values.rows
            form.setFieldValue("notice", "")
            if (recoveryOnly) {
                commands = uniqueCommands(
                    current.filter((row) => row.status === "UNKNOWN"),
                )
                if (!commands.length) throw new Error("没有可确认的待定结果")
                persist(current)
                let rows = current
                for (const commandPhase of ["prepare", "submit"] as const) {
                    const selected = uniqueCommands(
                        current.filter(
                            (row) =>
                                row.status === "UNKNOWN" &&
                                (row.commandPhase ?? "prepare") ===
                                    commandPhase,
                        ),
                    )
                    if (!selected.length) continue
                    const result = await command.mutateAsync({
                        rows: selected,
                        validateOnly: false,
                        recoveryOnly: true,
                        phase: commandPhase,
                    })
                    rows = applyBatchResult(
                        rows,
                        result,
                        mode,
                        true,
                        commandPhase,
                    )
                    changeRows(rows)
                }
                form.setFieldValue(
                    "notice",
                    rows.some((row) => row.status === "UNKNOWN")
                        ? "部分结果仍待确认。请稍后按原内容恢复；原操作记录会防止重复处理。"
                        : "原处理结果已核对；已完成资料不会重复处理。",
                )
                return
            }
            if (current.some((row) => row.status === "UNKNOWN"))
                throw new Error("请先确认待定结果，再开始新的提交")
            let ready
            if (mode === "new_product" && phase === "submit") {
                const groups = new Map<string, BatchEditorRow[]>()
                for (const row of current.filter(
                    (item) => item.applicationId && item.status !== "SUCCEEDED",
                ))
                    groups.set(row.applicationId!, [
                        ...(groups.get(row.applicationId!) ?? []),
                        row,
                    ])
                const rows = [...current]
                const submitCommands: PortalBatchRow[] = []
                for (const [id, group] of groups) {
                    const application = await client.fetchQuery({
                        queryKey: portalKeys.application(id),
                        queryFn: () => portalApplication(id),
                        staleTime: 0,
                    })
                    if (["pending", "effective"].includes(application.status)) {
                        for (const row of group)
                            rows[
                                rows.findIndex(
                                    (item) => item.rowId === row.rowId,
                                )
                            ] = {
                                ...row,
                                status: "SUCCEEDED",
                                message: "原申请已提交，待采购确认",
                                applicationVersion: application.version,
                            }
                        continue
                    }
                    const original =
                        group[0].commandPhase === "submit" &&
                        !["FAILED", "INVALID"].includes(group[0].status)
                            ? group[0].command
                            : undefined
                    const item = original ?? {
                        row_id: group[0].rowId,
                        idempotency_key: commandKey("batch-submit"),
                        input: { id, expected_version: application.version },
                    }
                    submitCommands.push(item)
                    for (const row of group)
                        rows[
                            rows.findIndex(
                                (candidate) => candidate.rowId === row.rowId,
                            )
                        ] = {
                            ...row,
                            command: item,
                            commandPhase: "submit",
                            applicationVersion: application.version,
                            status: "EDITING",
                            fieldErrors: {},
                        }
                }
                ready = { rows, commands: submitCommands }
            } else ready = await prepared.mutateAsync(current)
            changeRows(ready.rows)
            if (ready.rows.some((row) => row.status === "INVALID"))
                throw new Error(
                    "整批校验未通过，请修正异常行或明确移除后重试；本次尚未开始提交",
                )
            commands = ready.commands
            if (!commands.length) {
                form.setFieldValue(
                    "notice",
                    "本批资料已全部完成。可开始下一批。 ",
                )
                return
            }
            const validation = await command.mutateAsync({
                rows: commands,
                validateOnly: true,
                phase,
            })
            const checked = applyBatchResult(
                ready.rows,
                validation,
                mode,
                false,
                phase,
            )
            changeRows(checked)
            if (
                validation.valid === false ||
                checked.some(
                    (row) =>
                        !["SUCCEEDED", "PREPARED", "READY"].includes(
                            row.status,
                        ),
                )
            )
                throw new Error(
                    "整批校验未通过或尚未确认，请核对逐行结果后继续；本次未开始新的提交",
                )
            if (validateOnly) {
                form.setFieldValue(
                    "notice",
                    mode === "new_product" && phase === "prepare"
                        ? "整批校验通过；准备新品草稿时将再次核对当前资料。"
                        : "整批校验通过；点击提交后将再次核对当前资料。",
                )
                return
            }
            commands = uniqueCommands(
                checked.filter((row) => row.status === "READY"),
            )
            if (!commands.length) {
                form.setFieldValue("notice", "原处理已完成，已恢复处理结果。")
                return
            }
            // 先保存完整原命令并锁定，再发写请求；保存失败则禁止写入。
            const pending = markBatchUnknown(
                checked.map((row) =>
                    row.command &&
                    commands.some((item) => item.row_id === row.command!.row_id)
                        ? { ...row, commandPhase: phase }
                        : row,
                ),
                commands,
            )
            persist(pending)
            form.setFieldValue("rows", pending)
            writeStarted = true
            const result = await command.mutateAsync({
                rows: commands,
                validateOnly: false,
                phase,
            })
            const rows = applyBatchResult(pending, result, mode, false, phase)
            changeRows(rows)
            form.setFieldValue(
                "notice",
                rows.every((row) => row.status === "SUCCEEDED")
                    ? mode === "availability"
                        ? "本批可供情况已全部更新。"
                        : mode === "new_product"
                          ? "本批原新品申请已提交，待采购确认。"
                          : "本批申请已创建，待采购确认。当前价格和条款在确认前继续有效。"
                    : mode === "new_product" && phase === "prepare"
                      ? "新品草稿已准备。请在本批次补齐图片和资料后，核对并批量提交。"
                      : "已保留逐行结果，请处理未完成资料；待确认资料须先确认原结果。",
            )
        } catch (cause) {
            if (recoveryOnly) {
                // 整批恢复请求被拒绝不能证明上次各行没有提交；仅逐行终态结果可解锁。
                form.setFieldValue(
                    "notice",
                    `原结果尚未全部确认，原内容及操作记录继续保留。请按提示处理后恢复原操作。\n${getErrorMessage(cause, "暂时无法读取原结果，请稍后重试")}`,
                )
                return
            }
            if (writeStarted) {
                if (commandFailureDisposition(cause) === "unknown")
                    changeRows(
                        markBatchUnknown(form.state.values.rows, commands),
                    )
                else {
                    const ids = new Set(commands.map((item) => item.row_id))
                    changeRows(
                        form.state.values.rows.map((row) =>
                            row.status === "UNKNOWN" &&
                            row.command &&
                            ids.has(row.command.row_id)
                                ? {
                                      ...row,
                                      status: "FAILED",
                                      message: getErrorMessage(cause),
                                      command:
                                          commandFailureDisposition(cause) ===
                                          "rejected"
                                              ? undefined
                                              : row.command,
                                  }
                                : row,
                        ),
                    )
                }
            }
            form.setFieldValue(
                "notice",
                writeStarted && commandFailureDisposition(cause) === "unknown"
                    ? mode === "new_product" && phase === "prepare"
                        ? "暂时无法确认保存草稿结果。原内容已保留，请确认并恢复原操作。"
                        : "暂时无法确认提交结果。原内容已保留，请确认并恢复原操作。"
                    : getErrorMessage(cause, "操作未完成，请核对后重试"),
            )
        } finally {
            running.current = false
        }
    }
    function requestClose() {
        if (busy || running.current) return
        const rows = form.state.values.rows
        try {
            if (rows.length && !rows.every((row) => row.status === "SUCCEEDED"))
                persist(rows)
            else if (!form.state.values.recoveryBlocked)
                sessionStorage.removeItem(storageKey)
        } catch {
            if (rows.some((row) => row.status === "UNKNOWN")) {
                form.setFieldValue(
                    "notice",
                    "无法保存待确认记录，请先确认待定结果后再关闭。 ",
                )
                return
            }
        }
        onClose()
    }
    const columns = batchColumns(mode)
    const mainColumns = mainBatchColumns(mode)
    const extraColumns = columns.filter(
        (item) => !mainColumns.some((main) => main.key === item.key),
    )
    function cell(row: BatchEditorRow, rowIndex: number, item: BatchColumn) {
        const id = `${prefix}-row-${toAutomationIdSegment(row.rowId)}-${toAutomationIdSegment(item.key)}`
        const disabled =
            busy ||
            readOnly ||
            isBatchRowLocked(row) ||
            form.state.values.recoveryBlocked
        return (
            <form.AppField
                key={item.key}
                name={`rows[${rowIndex}].values.${item.key}`}
            >
                {(field) =>
                    item.key === "availability" ||
                    item.key === "productKind" ? (
                        <field.SelectField
                            id={id}
                            label={item.label}
                            hideLabel
                            options={
                                item.key === "availability"
                                    ? [
                                          { value: "有货", label: "有货" },
                                          {
                                              value: "临时缺货",
                                              label: "临时缺货",
                                          },
                                      ]
                                    : [
                                          { value: "实物", label: "实物" },
                                          { value: "虚拟", label: "虚拟" },
                                          {
                                              value: "线下服务",
                                              label: "线下服务",
                                          },
                                          { value: "卡券", label: "卡券" },
                                      ]
                            }
                            allowClear={false}
                            disabled={disabled}
                            onValueChange={(value) =>
                                changeCell(row.rowId, item.key, value)
                            }
                        />
                    ) : (
                        <Input
                            id={id}
                            aria-label={item.label}
                            aria-invalid={
                                !!row.fieldErrors[item.key] || undefined
                            }
                            aria-describedby={
                                row.message
                                    ? `${prefix}-row-${toAutomationIdSegment(row.rowId)}-result`
                                    : undefined
                            }
                            value={field.state.value}
                            disabled={disabled}
                            className="min-w-36"
                            onBlur={field.handleBlur}
                            onChange={(event) =>
                                changeCell(
                                    row.rowId,
                                    item.key,
                                    event.target.value,
                                )
                            }
                            onPaste={(event) =>
                                pasteCells(event, row.rowId, item.key)
                            }
                        />
                    )
                }
            </form.AppField>
        )
    }
    function renderTable(rows: BatchEditorRow[]) {
        return (
            <div className="overflow-x-auto rounded-lg border">
                <Table className="w-full text-sm">
                    <TableHeader className="bg-muted/40">
                        <TableRow>
                            <TableHead className="min-w-48 px-3 py-3 text-left font-medium">
                                对象及结果
                            </TableHead>
                            {mainColumns.map((item) => (
                                <TableHead
                                    key={item.key}
                                    className="min-w-36 px-2 py-3 text-left font-medium"
                                >
                                    {item.label}
                                </TableHead>
                            ))}
                            <TableHead className="px-3 py-3 text-left font-medium">
                                操作
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {rows.length === 0 && (
                            <TableRow>
                                <TableCell
                                    colSpan={mainColumns.length + 2}
                                    className="p-8 text-center text-muted-foreground"
                                >
                                    下载模板后导入，或添加一行手动填写。
                                </TableCell>
                            </TableRow>
                        )}
                        {rows.map((row, index) => {
                            const rowPrefix = `${prefix}-row-${toAutomationIdSegment(row.rowId)}`
                            return (
                                <BatchRowFragment
                                    key={row.rowId}
                                    identity={
                                        <>
                                            <p className="font-medium">
                                                {row.matchedName ||
                                                    (mode === "new_product"
                                                        ? row.values.skuName
                                                        : row.values.skuNo) ||
                                                    `第${index + 1}条资料`}
                                            </p>
                                            <p
                                                className={`mt-1 text-xs ${row.status === "INVALID" || row.status === "FAILED" ? "text-destructive" : "text-muted-foreground"}`}
                                            >
                                                {row.status === "SUCCEEDED"
                                                    ? mode === "availability"
                                                        ? "已更新可供"
                                                        : mode === "new_product"
                                                          ? "待采购确认"
                                                          : "待采购确认"
                                                    : batchStatusLabels[
                                                          row.status
                                                      ]}
                                            </p>
                                        </>
                                    }
                                    cells={mainColumns.map((item) => (
                                        <TableCell
                                            key={item.key}
                                            className="px-2 py-3 align-top"
                                        >
                                            {cell(row, index, item)}
                                        </TableCell>
                                    ))}
                                    actions={
                                        <Button
                                            id={`${rowPrefix}-remove`}
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            disabled={
                                                busy ||
                                                readOnly ||
                                                isBatchRowLocked(row)
                                            }
                                            onClick={() => {
                                                const remaining = rows.filter(
                                                    (item) =>
                                                        item.rowId !==
                                                        row.rowId,
                                                )
                                                const affected =
                                                    mode === "new_product"
                                                        ? new Map(
                                                              remaining
                                                                  .filter(
                                                                      (item) =>
                                                                          item
                                                                              .values
                                                                              .groupCode ===
                                                                              row
                                                                                  .values
                                                                                  .groupCode &&
                                                                          !isBatchRowLocked(
                                                                              item,
                                                                          ),
                                                                  )
                                                                  .map(
                                                                      (
                                                                          item,
                                                                      ) => [
                                                                          item.rowId,
                                                                          {},
                                                                      ],
                                                                  ),
                                                          )
                                                        : new Map<
                                                              string,
                                                              Partial<BatchValues>
                                                          >()
                                                changeRows(
                                                    editBatchValues(
                                                        remaining,
                                                        affected,
                                                        mode,
                                                    ),
                                                )
                                            }}
                                        >
                                            移除此行
                                        </Button>
                                    }
                                    details={
                                        <div className="space-y-2">
                                            {mode === "new_product" &&
                                                row.applicationId &&
                                                ![
                                                    "SUCCEEDED",
                                                    "UNKNOWN",
                                                ].includes(row.status) && (
                                                    <Button
                                                        id={`${rowPrefix}-edit-draft`}
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={busy}
                                                        onClick={() =>
                                                            setEditingDraft(
                                                                row.applicationId!,
                                                            )
                                                        }
                                                    >
                                                        补充原草稿图片及资料
                                                    </Button>
                                                )}
                                            {row.status === "SUCCEEDED" &&
                                                row.applicationId && (
                                                    <Link
                                                        id={`${rowPrefix}-application`}
                                                        href={`/supplier-portal/applications/${encodeURIComponent(row.applicationId)}`}
                                                        className="inline-flex rounded-md px-2 py-1 text-xs font-medium text-primary underline underline-offset-4"
                                                    >
                                                        {mode === "new_product"
                                                            ? "查看原申请"
                                                            : "查看申请"}
                                                    </Link>
                                                )}
                                            {row.message && (
                                                <p
                                                    id={`${rowPrefix}-result`}
                                                    className={`text-xs ${row.status === "INVALID" || row.status === "FAILED" ? "text-destructive" : "text-muted-foreground"}`}
                                                >
                                                    {row.message}
                                                </p>
                                            )}
                                            {extraColumns.length > 0 && (
                                                <details
                                                    id={`${rowPrefix}-extra`}
                                                >
                                                    <summary
                                                        id={`${rowPrefix}-expand`}
                                                        className="cursor-pointer text-xs font-medium"
                                                    >
                                                        {mode === "new_product"
                                                            ? "公共资料、规格及其他条款"
                                                            : "数量、区域、日期及其他条款"}
                                                    </summary>
                                                    <div className="mt-3 grid gap-3 md:grid-cols-3">
                                                        {extraColumns.map(
                                                            (item) => (
                                                                <div
                                                                    key={
                                                                        item.key
                                                                    }
                                                                >
                                                                    <label
                                                                        htmlFor={`${rowPrefix}-${toAutomationIdSegment(item.key)}`}
                                                                        className="mb-1 block text-xs text-muted-foreground"
                                                                    >
                                                                        {
                                                                            item.label
                                                                        }
                                                                    </label>
                                                                    {cell(
                                                                        row,
                                                                        index,
                                                                        item,
                                                                    )}
                                                                </div>
                                                            ),
                                                        )}
                                                    </div>
                                                </details>
                                            )}
                                            {mode !== "new_product" &&
                                                !isBatchRowLocked(row) && (
                                                    <Button
                                                        id={`${rowPrefix}-reload`}
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={
                                                            busy || readOnly
                                                        }
                                                        onClick={() =>
                                                            void reloadRow(row)
                                                        }
                                                    >
                                                        读取当前资料，保留本行输入
                                                    </Button>
                                                )}
                                            {mode !== "new_product" &&
                                                !isBatchRowLocked(row) &&
                                                reviewedTargets[row.rowId] && (
                                                    <Button
                                                        id={`${rowPrefix}-confirm-current`}
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={
                                                            busy || readOnly
                                                        }
                                                        onClick={() => {
                                                            const verified =
                                                                reviewedTargets[
                                                                    row.rowId
                                                                ]
                                                            const current =
                                                                form.state.values.rows.find(
                                                                    (item) =>
                                                                        item.rowId ===
                                                                        row.rowId,
                                                                )
                                                            if (
                                                                !current ||
                                                                current.key !==
                                                                    verified.key
                                                            ) {
                                                                form.setFieldValue(
                                                                    "notice",
                                                                    "本行输入已变化，请重新读取当前资料再核对",
                                                                )
                                                                return
                                                            }
                                                            const edited =
                                                                editBatchValues(
                                                                    [current],
                                                                    new Map([
                                                                        [
                                                                            current.rowId,
                                                                            {},
                                                                        ],
                                                                    ]),
                                                                    mode,
                                                                )
                                                            const updated =
                                                                prepareBatchRows(
                                                                    edited,
                                                                    mode,
                                                                    new Map([
                                                                        [
                                                                            current.rowId,
                                                                            verified.target,
                                                                        ],
                                                                    ]),
                                                                ).rows[0]
                                                            changeRows(
                                                                form.state.values.rows.map(
                                                                    (item) =>
                                                                        item.rowId ===
                                                                        current.rowId
                                                                            ? {
                                                                                  ...updated,
                                                                                  message:
                                                                                      updated.status ===
                                                                                      "INVALID"
                                                                                          ? updated.message
                                                                                          : "已核对当前目标，原输入继续保留；请校验后提交",
                                                                              }
                                                                            : item,
                                                                ),
                                                            )
                                                            setReviewedTargets(
                                                                (values) => {
                                                                    const copy =
                                                                        {
                                                                            ...values,
                                                                        }
                                                                    delete copy[
                                                                        row
                                                                            .rowId
                                                                    ]
                                                                    return copy
                                                                },
                                                            )
                                                        }}
                                                    >
                                                        已核对，保留输入重新准备
                                                    </Button>
                                                )}
                                        </div>
                                    }
                                    colSpan={mainColumns.length + 2}
                                />
                            )
                        })}
                    </TableBody>
                </Table>
            </div>
        )
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
                    showCloseButton={false}
                    className="flex max-h-[90svh] flex-col gap-4 sm:max-w-6xl"
                >
                    <DialogHeader>
                        <div className="flex items-center justify-between gap-3">
                            <DialogTitle>{batchModeLabels[mode]}</DialogTitle>
                            <Button
                                id={`${prefix}-close`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={busy}
                                onClick={requestClose}
                            >
                                关闭
                            </Button>
                        </div>
                        <DialogDescription>
                            {mode === "availability"
                                ? "整批校验通过后逐行更新可供；有货不会解除采购暂停或停止关系。"
                                : mode === "new_product"
                                  ? "先按商品组准备草稿，在本批次内补齐图片及资料，然后核对并批量提交采购确认。"
                                  : "整批校验通过后创建待采购确认的申请。确认前，当前价格与条款继续有效。"}
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex min-h-0 flex-1 flex-col gap-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
                            <form.Subscribe selector={(state) => state.values}>
                                {(values) => {
                                    const unknown = values.rows.filter(
                                        (row) => row.status === "UNKNOWN",
                                    )
                                    const unfinished = values.rows.filter(
                                        (row) => row.status !== "SUCCEEDED",
                                    )
                                    return (
                                        <>
                                            {values.notice && (
                                                <p
                                                    role="status"
                                                    className="whitespace-pre-line rounded-lg border bg-muted/30 p-3 text-sm"
                                                >
                                                    {values.notice}
                                                </p>
                                            )}
                                            {values.recoveryBlocked ? (
                                                <div className="space-y-3 rounded-lg border p-4">
                                                    <p>
                                                        先核对原申请或可供结果。确认原结果后，可清理无法读取的浏览器记录并重新整理资料。
                                                    </p>
                                                    <Button
                                                        id={`${prefix}-clear-unreadable`}
                                                        type="button"
                                                        variant="outline"
                                                        onClick={() => {
                                                            try {
                                                                sessionStorage.removeItem(
                                                                    storageKey,
                                                                )
                                                                form.setFieldValue(
                                                                    "recoveryBlocked",
                                                                    false,
                                                                )
                                                                form.setFieldValue(
                                                                    "notice",
                                                                    "已清理本账号无法读取的记录，请按已核对的结果继续。",
                                                                )
                                                            } catch {
                                                                form.setFieldValue(
                                                                    "notice",
                                                                    "浏览器记录暂时无法清理，请检查会话存储。",
                                                                )
                                                            }
                                                        }}
                                                    >
                                                        已核对原结果，清理记录
                                                    </Button>
                                                </div>
                                            ) : (
                                                <>
                                                    <section className="space-y-3 rounded-lg border p-4">
                                                        <div className="flex flex-wrap items-center justify-between gap-3">
                                                            <p className="text-sm font-medium">
                                                                从表格加入资料
                                                            </p>
                                                            <LoadingButton
                                                                id={`${prefix}-template`}
                                                                type="button"
                                                                variant="outline"
                                                                size="sm"
                                                                loading={
                                                                    template.isPending
                                                                }
                                                                onClick={() => {
                                                                    void template
                                                                        .mutateAsync()
                                                                        .catch(
                                                                            (
                                                                                cause,
                                                                            ) =>
                                                                                form.setFieldValue(
                                                                                    "notice",
                                                                                    getErrorMessage(
                                                                                        cause,
                                                                                        "模板暂时无法下载，请重试",
                                                                                    ),
                                                                                ),
                                                                        )
                                                                }}
                                                            >
                                                                下载本次模板
                                                            </LoadingButton>
                                                        </div>
                                                        <form.AppField name="paste">
                                                            {(field) => (
                                                                <field.TextareaField
                                                                    id={`${prefix}-paste`}
                                                                    label="粘贴表格（含本次模板表头）"
                                                                    rows={3}
                                                                    placeholder={columns
                                                                        .map(
                                                                            (
                                                                                item,
                                                                            ) =>
                                                                                item.label,
                                                                        )
                                                                        .join(
                                                                            "\t",
                                                                        )}
                                                                    disabled={
                                                                        busy ||
                                                                        readOnly ||
                                                                        unknown.length >
                                                                            0
                                                                    }
                                                                />
                                                            )}
                                                        </form.AppField>
                                                        <div className="flex flex-wrap items-center gap-3">
                                                            <LoadingButton
                                                                id={`${prefix}-import-paste`}
                                                                type="button"
                                                                variant="outline"
                                                                size="sm"
                                                                loading={
                                                                    imported.isPending
                                                                }
                                                                disabled={
                                                                    busy ||
                                                                    readOnly ||
                                                                    unknown.length >
                                                                        0 ||
                                                                    !values.paste.trim()
                                                                }
                                                                onClick={() =>
                                                                    void importRows(
                                                                        values.paste,
                                                                    )
                                                                }
                                                            >
                                                                加入粘贴资料
                                                            </LoadingButton>
                                                            <label
                                                                htmlFor={`${prefix}-file`}
                                                                className="text-sm"
                                                            >
                                                                导入Excel或CSV
                                                            </label>
                                                            <input
                                                                id={`${prefix}-file`}
                                                                type="file"
                                                                accept=".xlsx,.csv"
                                                                className="min-w-0 max-w-64 text-xs"
                                                                disabled={
                                                                    busy ||
                                                                    readOnly ||
                                                                    unknown.length >
                                                                        0
                                                                }
                                                                onChange={(
                                                                    event,
                                                                ) => {
                                                                    const file =
                                                                        event
                                                                            .target
                                                                            .files?.[0]
                                                                    event.target.value =
                                                                        ""
                                                                    if (file)
                                                                        void importRows(
                                                                            file,
                                                                        )
                                                                }}
                                                            />
                                                        </div>
                                                        <p className="text-xs text-muted-foreground">
                                                            最多100条SKU或供给数据、5
                                                            MB；编号按文本填写。可供数量空白为未提供，0为明确为零。单元格多格粘贴按模板列顺序，超出范围或包含锁定行时整次拒绝。
                                                        </p>
                                                        {mode ===
                                                            "new_product" && (
                                                            <p className="text-xs text-muted-foreground">
                                                                一行一个SKU，同一商品使用同一商品组编号并保持公共资料一致。品牌、分类和单位可填写待内部匹配的原始值；保存按整组处理。图片不随表格导入，保存后在原草稿上传，再提交采购确认。
                                                            </p>
                                                        )}
                                                    </section>
                                                    <div className="flex flex-wrap items-center justify-between gap-3">
                                                        <p className="text-sm">
                                                            共
                                                            {values.rows.length}
                                                            条数据 · 待处理
                                                            {batchProcessingCount(
                                                                values.rows,
                                                                mode,
                                                            )}
                                                            {mode ===
                                                            "new_product"
                                                                ? "个商品组"
                                                                : "行"}{" "}
                                                            · 已完成
                                                            {values.rows
                                                                .length -
                                                                unfinished.length}
                                                            行
                                                        </p>
                                                        <div className="flex gap-2">
                                                            {values.rows
                                                                .length > 0 &&
                                                                !unfinished.length && (
                                                                    <Button
                                                                        id={`${prefix}-next-batch`}
                                                                        type="button"
                                                                        variant="outline"
                                                                        size="sm"
                                                                        disabled={
                                                                            busy ||
                                                                            readOnly
                                                                        }
                                                                        onClick={() => {
                                                                            changeRows(
                                                                                [],
                                                                            )
                                                                            form.setFieldValue(
                                                                                "notice",
                                                                                "可加入下一批资料。",
                                                                            )
                                                                        }}
                                                                    >
                                                                        开始下一批
                                                                    </Button>
                                                                )}
                                                            <Button
                                                                id={`${prefix}-add-row`}
                                                                type="button"
                                                                variant="outline"
                                                                size="sm"
                                                                disabled={
                                                                    busy ||
                                                                    readOnly ||
                                                                    unknown.length >
                                                                        0 ||
                                                                    values.rows
                                                                        .length >=
                                                                        PORTAL_BATCH_LIMIT
                                                                }
                                                                onClick={() =>
                                                                    changeRows([
                                                                        ...values.rows,
                                                                        newBatchRow(),
                                                                    ])
                                                                }
                                                            >
                                                                添加一行
                                                            </Button>
                                                        </div>
                                                    </div>
                                                    {renderTable(values.rows)}
                                                </>
                                            )}
                                        </>
                                    )
                                }}
                            </form.Subscribe>
                        </div>
                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => {
                                const unknown = values.rows.some(
                                    (row) => row.status === "UNKNOWN",
                                )
                                const pending = values.rows.some(
                                    (row) => !isBatchRowLocked(row),
                                )
                                const disabled =
                                    busy ||
                                    readOnly ||
                                    !values.hydrated ||
                                    values.recoveryBlocked
                                return (
                                    <DialogFooter className="shrink-0 flex-wrap border-t pt-4">
                                        {values.rows.some(
                                            (row) =>
                                                row.status === "INVALID" ||
                                                row.status === "FAILED" ||
                                                row.status === "UNKNOWN",
                                        ) && (
                                            <Button
                                                id={`${prefix}-export-errors`}
                                                type="button"
                                                variant="ghost"
                                                disabled={busy}
                                                onClick={() =>
                                                    downloadPortalBatchErrors(
                                                        values.rows,
                                                        mode,
                                                    )
                                                }
                                            >
                                                导出未完成明细
                                            </Button>
                                        )}
                                        <Button
                                            id={`${prefix}-cancel`}
                                            type="button"
                                            variant="outline"
                                            disabled={busy}
                                            onClick={requestClose}
                                        >
                                            {unknown
                                                ? "关闭并保留记录"
                                                : "关闭"}
                                        </Button>
                                        {unknown && (
                                            <LoadingButton
                                                id={`${prefix}-recover`}
                                                type="button"
                                                loading={
                                                    command.isPending &&
                                                    !!command.variables
                                                        ?.recoveryOnly
                                                }
                                                disabled={disabled}
                                                onClick={() =>
                                                    void run(false, true)
                                                }
                                            >
                                                确认并恢复原操作
                                            </LoadingButton>
                                        )}
                                        {mode === "new_product" &&
                                            values.rows.some(
                                                (row) =>
                                                    row.applicationId &&
                                                    row.status !== "SUCCEEDED",
                                            ) && (
                                                <LoadingButton
                                                    id={`${prefix}-submit-prepared`}
                                                    type="button"
                                                    disabled={
                                                        disabled || unknown
                                                    }
                                                    loading={
                                                        command.isPending &&
                                                        command.variables
                                                            ?.phase === "submit"
                                                    }
                                                    onClick={() =>
                                                        void run(
                                                            false,
                                                            false,
                                                            "submit",
                                                        )
                                                    }
                                                >
                                                    核对并批量提交原草稿
                                                </LoadingButton>
                                            )}
                                        <LoadingButton
                                            id={`${prefix}-validate`}
                                            type="button"
                                            variant="outline"
                                            loading={
                                                prepared.isPending ||
                                                (command.isPending &&
                                                    !!command.variables
                                                        ?.validateOnly)
                                            }
                                            disabled={
                                                disabled || unknown || !pending
                                            }
                                            onClick={() => void run(true)}
                                        >
                                            校验整批
                                        </LoadingButton>
                                        <LoadingButton
                                            id={`${prefix}-submit`}
                                            type="submit"
                                            loading={
                                                command.isPending &&
                                                !command.variables
                                                    ?.validateOnly &&
                                                !command.variables?.recoveryOnly
                                            }
                                            disabled={
                                                disabled || unknown || !pending
                                            }
                                        >
                                            {mode === "new_product"
                                                ? "准备新品草稿 · "
                                                : "校验并提交"}
                                            {batchProcessingCount(
                                                values.rows,
                                                mode,
                                            )}
                                            {mode === "new_product"
                                                ? "个商品组"
                                                : "行"}
                                        </LoadingButton>
                                    </DialogFooter>
                                )
                            }}
                        </form.Subscribe>
                    </form>
                </DialogContent>
            </Dialog>
            {editingDraft && (
                <PortalBatchDraftEditor
                    id={editingDraft}
                    onClose={() => setEditingDraft(null)}
                    onSaved={(application: PortalApplication) => {
                        changeRows(
                            form.state.values.rows.map((row) =>
                                row.applicationId === application.id
                                    ? {
                                          ...row,
                                          applicationVersion:
                                              application.version,
                                          command: undefined,
                                          commandPhase: "prepare",
                                          status: "PREPARED",
                                          fieldErrors: {},
                                          message:
                                              "资料已保存，可在本批次统一提交",
                                      }
                                    : row,
                            ),
                        )
                        setEditingDraft(null)
                    }}
                />
            )}
        </>
    )
}

function BatchRowFragment({
    identity,
    cells,
    actions,
    details,
    colSpan,
}: {
    identity: React.ReactNode
    cells: React.ReactNode
    actions: React.ReactNode
    details: React.ReactNode
    colSpan: number
}) {
    return (
        <>
            <TableRow className="border-t">
                <TableCell className="px-3 py-3 align-top whitespace-normal">
                    {identity}
                </TableCell>
                {cells}
                <TableCell className="px-3 py-3 align-top">{actions}</TableCell>
            </TableRow>
            <TableRow>
                <TableCell
                    colSpan={colSpan}
                    className="px-3 pb-4 whitespace-normal"
                >
                    {details}
                </TableCell>
            </TableRow>
        </>
    )
}
