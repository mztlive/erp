"use client"
import { useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { ApiErrorException, getErrorMessage } from "@/lib/api/errors"
import { useSaveCompanyMutation } from "./queries"
import type { Company } from "./api"

export const CompanyForm = ({
    company,
    onClose,
}: {
    company?: Company
    onClose: () => void
}) => {
    const mutation = useSaveCompanyMutation()
    const [partyNo] = useState(
        () =>
            company?.party_no ??
            `PTY-${Date.now().toString(36).toUpperCase()}${crypto.randomUUID().replaceAll("-", "").slice(0, 6).toUpperCase()}`,
    )
    const [error, setError] = useState("")
    const [uncertain, setUncertain] = useState(false)
    const locked = mutation.isPending || uncertain
    const schema = z.object({
        legalName: z.string().trim().min(1, "请输入公司全称").max(256),
        shortName: z.string().max(128),
        aliases: z.string(),
        creditCode: z
            .string()
            .trim()
            .refine(
                (s) => !s || /^[0-9A-Z]{18}$/i.test(s),
                "请输入 18 位统一社会信用代码",
            ),
    })
    const form = useAppForm({
        defaultValues: {
            legalName: company?.legal_name ?? "",
            shortName: company?.short_name ?? "",
            aliases: company?.aliases.join("、") ?? "",
            creditCode: company?.unified_credit_code ?? "",
        },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            setError("")
            try {
                await mutation.mutateAsync({
                    id: company?.id,
                    input: {
                        party_no: partyNo,
                        version: company?.version,
                        legal_name: value.legalName.trim(),
                        short_name: value.shortName.trim() || null,
                        aliases: value.aliases
                            .split(/[、,，\n]/)
                            .map((s) => s.trim())
                            .filter(Boolean),
                        unified_credit_code:
                            value.creditCode.trim().toUpperCase() || null,
                        status: company?.status ?? "active",
                    },
                })
                onClose()
            } catch (err) {
                const unknown = !(
                    err instanceof ApiErrorException &&
                    typeof err.status === "number" &&
                    err.status >= 400 &&
                    err.status < 500 &&
                    err.status !== 408
                )
                setUncertain(unknown)
                setError(
                    getErrorMessage(err, "保存失败，请重试") +
                        (unknown
                            ? "。结果暂无法确认，请保持当前内容重试核对。"
                            : ""),
                )
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !locked) onClose()
            }}
        >
            <DialogContent
                closeButtonId="company-form-dismiss"
                id="company-form-dialog"
                className="sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {company ? "编辑公司主体" : "新建公司主体"}
                    </DialogTitle>
                    <DialogDescription>
                        用于供应商签约、付款及导入匹配。别名用于匹配导入表内简称。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="legalName">
                        {(field) => (
                            <field.TextField
                                disabled={locked}
                                id="company-form-legal-name"
                                label="公司全称"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="shortName">
                        {(field) => (
                            <field.TextField
                                disabled={locked}
                                id="company-form-short-name"
                                label="公司简称"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="aliases">
                        {(field) => (
                            <field.TextField
                                disabled={locked}
                                id="company-form-aliases"
                                label="导入别名（多个用顿号分隔）"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="creditCode">
                        {(field) => (
                            <field.TextField
                                disabled={locked}
                                id="company-form-credit-code"
                                label="统一社会信用代码"
                            />
                        )}
                    </form.AppField>
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    )}
                    <div className="flex justify-end gap-2">
                        <Button
                            id="company-form-cancel"
                            type="button"
                            variant="outline"
                            disabled={locked}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <Button
                            id="company-form-submit"
                            type="submit"
                            disabled={mutation.isPending}
                        >
                            {mutation.isPending
                                ? "保存中…"
                                : uncertain
                                  ? "重试核对"
                                  : "保存"}
                        </Button>
                    </div>
                </form>
            </DialogContent>
        </Dialog>
    )
}
