"use client"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useContractUploadForm,
    type UseContractUploadFormOptions,
} from "@/features/contracts/hooks/use-contract-upload-form"

export type ContractUploadDialogProps = UseContractUploadFormOptions
const STATUS = {
    ready: "等待识别",
    processing: "正在识别",
    failed: "导入失败",
    succeeded: "已归档",
}
const FIELDS: Record<string, string> = {
    contract_no: "合同编号",
    customer_name: "对方签约名称",
    customer_credit_code: "对方信用代码",
    company_name: "我方签约名称",
    company_credit_code: "我方信用代码",
    settlement_name: "结算主体",
    settlement_credit_code: "结算主体信用代码",
    payment_terms: "付款条件",
    invoice_type: "开票要求",
    tax_point: "税率",
    signed_at: "签订日期",
    valid_from: "有效期起",
    valid_to: "有效期止",
    business_scope: "业务范围",
}

export function ContractUploadDialog(props: ContractUploadDialogProps) {
    const state = useContractUploadForm(props)
    const task = state.detail.data
    const busy = state.uploadMutation.isPending || state.runMutation.isPending
    const error =
        state.uploadMutation.error ||
        state.runMutation.error ||
        state.list.error ||
        state.detail.error ||
        state.previewMutation.error
    const processing = task?.status === "processing"
    const recoveryAvailable =
        processing &&
        task.recoverable_at != null &&
        state.detail.dataUpdatedAt >= task.recoverable_at * 1000
    return (
        <Dialog open={props.open} onOpenChange={props.onOpenChange}>
            <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden sm:max-w-4xl">
                <DialogHeader>
                    <DialogTitle>
                        {props.revisionTarget
                            ? "识别合同新版本"
                            : "上传合同 PDF"}
                    </DialogTitle>
                    <DialogDescription>
                        逐页识别并匹配已有客户与主体。业务字段由原文提取，不可手工修改；校验全部通过后归档。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 space-y-4 overflow-y-auto">
                    {error ? (
                        <Alert variant="destructive">
                            <AlertTitle>操作未完成</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(error, "请查看任务结果后重试")}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    <form
                        onSubmit={(event) => {
                            event.preventDefault()
                            void state.form.handleSubmit()
                        }}
                        className="space-y-3"
                    >
                        <state.form.AppField name="pdfFile">
                            {(field) => (
                                <field.PdfUploadField
                                    id="card-contracts-upload-pdf"
                                    label="已签署合同 PDF"
                                />
                            )}
                        </state.form.AppField>
                        <state.form.AppForm>
                            <state.form.SubmitButton
                                id="card-contracts-upload-submit"
                                loading={busy}
                                label={busy ? "正在处理…" : "上传并识别"}
                            />
                        </state.form.AppForm>
                    </form>
                    <div className="space-y-2 border-t pt-3">
                        <p className="text-sm font-medium">我的导入记录</p>
                        {state.list.isLoading ? (
                            <p className="text-sm text-muted-foreground">
                                加载中…
                            </p>
                        ) : null}
                        {state.list.data?.items.map((item) => (
                            <Button
                                id={`contract-import-select-${toAutomationIdSegment(item.id)}`}
                                key={item.id}
                                type="button"
                                variant={
                                    state.selectedId === item.id
                                        ? "secondary"
                                        : "ghost"
                                }
                                className="mr-2 max-w-full"
                                onClick={() => state.select(item.id)}
                            >
                                <span className="truncate">
                                    {item.file_name}
                                </span>
                                <span>{STATUS[item.status]}</span>
                            </Button>
                        ))}
                        <div className="flex gap-2">
                            <Button
                                id="contract-import-page-previous"
                                type="button"
                                variant="outline"
                                disabled={state.page <= 1}
                                onClick={() => state.setPage(state.page - 1)}
                            >
                                上一页
                            </Button>
                            <Button
                                id="contract-import-page-next"
                                type="button"
                                variant="outline"
                                disabled={
                                    !state.list.data ||
                                    state.page * 20 >= state.list.data.total
                                }
                                onClick={() => state.setPage(state.page + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    </div>
                    {task ? (
                        <section
                            className="space-y-3 rounded-lg border p-4"
                            aria-label="合同识别结果"
                        >
                            <p className="text-sm font-medium">
                                {task.file_name} · {task.page_count} 页 ·{" "}
                                {STATUS[task.status]}
                            </p>
                            {state.contextError ? (
                                <Alert variant="destructive">
                                    <AlertDescription>
                                        {state.contextError}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {task.failure ? (
                                <Alert variant="destructive">
                                    <AlertTitle>合同未归档</AlertTitle>
                                    <AlertDescription>
                                        {task.failure.field
                                            ? `${FIELDS[task.failure.field] ?? "合同字段"}：`
                                            : ""}
                                        {task.failure.page
                                            ? `第 ${task.failure.page} 页，`
                                            : ""}
                                        {task.failure.message}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            <div className="flex gap-2">
                                <Button
                                    id="contract-import-preview"
                                    type="button"
                                    variant="outline"
                                    disabled={state.previewMutation.isPending}
                                    onClick={() =>
                                        state.previewMutation.mutate(task.id)
                                    }
                                >
                                    查看原文
                                </Button>
                                {task.status !== "succeeded" ? (
                                    <Button
                                        id="contract-import-retry"
                                        type="button"
                                        variant="outline"
                                        disabled={
                                            busy ||
                                            Boolean(state.contextError) ||
                                            (processing && !recoveryAvailable)
                                        }
                                        onClick={state.retry}
                                    >
                                        {processing
                                            ? "恢复识别"
                                            : "重新识别并匹配"}
                                    </Button>
                                ) : null}
                                <Button
                                    id="contract-import-refresh"
                                    type="button"
                                    variant="ghost"
                                    onClick={() => void state.detail.refetch()}
                                >
                                    刷新结果
                                </Button>
                            </div>
                            {processing ? (
                                <p className="text-sm text-muted-foreground">
                                    正在处理，可稍后重新打开此窗口查看。处理中断满
                                    10 分钟后可恢复识别。
                                </p>
                            ) : null}
                            {task.extraction ? (
                                <dl className="grid gap-3 sm:grid-cols-2">
                                    {Object.entries(task.extraction.fields).map(
                                        ([key, field]) => (
                                            <div
                                                key={key}
                                                className="min-w-0 rounded-md bg-muted/40 p-3"
                                            >
                                                <dt className="text-xs text-muted-foreground">
                                                    {FIELDS[key] ?? "合同字段"}
                                                </dt>
                                                <dd className="break-words text-sm">
                                                    {field.value}
                                                </dd>
                                                <dd className="mt-1 break-words text-xs text-muted-foreground">
                                                    第 {field.page} 页：
                                                    {field.quote}
                                                </dd>
                                            </div>
                                        ),
                                    )}
                                </dl>
                            ) : null}
                            {state.previewUrl ? (
                                <iframe
                                    title="导入合同原文"
                                    src={state.previewUrl}
                                    className="h-96 w-full rounded border"
                                />
                            ) : null}
                        </section>
                    ) : null}
                </div>
                <DialogFooter>
                    <Button
                        id="card-contracts-upload-cancel"
                        type="button"
                        variant="outline"
                        onClick={() => props.onOpenChange(false)}
                    >
                        关闭
                    </Button>
                    {task?.status === "succeeded" ? (
                        <Button
                            id="contract-import-use"
                            type="button"
                            disabled={!state.canAccept}
                            onClick={state.accept}
                        >
                            {props.onSuccess ? "使用此合同" : "完成"}
                        </Button>
                    ) : null}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
