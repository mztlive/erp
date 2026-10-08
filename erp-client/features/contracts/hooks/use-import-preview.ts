"use client"

import { useEffect, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { previewContractImport } from "../api/upload"

export function useImportPreview(taskId: string) {
    const query = useQuery({
        queryKey: ["contract-imports", "preview", taskId],
        queryFn: () => previewContractImport(taskId),
        staleTime: Infinity,
        gcTime: 0,
        retry: false,
    })
    const [source, setSource] = useState<{ blob: Blob; url: string }>()
    useEffect(() => {
        if (!query.data) return
        const url = URL.createObjectURL(query.data)
        setSource({ blob: query.data, url })
        return () => URL.revokeObjectURL(url)
    }, [query.data])
    return {
        ...query,
        url: source?.blob === query.data ? source?.url : undefined,
    }
}
