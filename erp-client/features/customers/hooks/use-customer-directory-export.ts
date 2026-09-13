"use client"
import { useMutation } from "@tanstack/react-query"
import { exportCustomerDirectory } from "../api/directory-export"
import type { CustomerDirectoryQuery } from "../types"
import { downloadListCsv } from "@/lib/list-export"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"

/** 只有完整读取成功才下载；错误保持在当前目录，允许重试。 */
export const useCustomerDirectoryExport = (query: CustomerDirectoryQuery) =>
    useMutation({
        mutationFn: () => exportCustomerDirectory(query),
        onSuccess: (content) => downloadListCsv(content, "客户目录.csv"),
        onError: (error) =>
            toast.add({
                title: "导出失败",
                description: getErrorMessage(error, "请重新查询后重试"),
                type: "error",
            }),
    })
