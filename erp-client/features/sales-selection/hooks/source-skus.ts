"use client"

import { useQuery } from "@tanstack/react-query"
import { fetchSourceSkus } from "../api/source-skus"
import { salesSelectionKeys } from "./queries"

export const useSourceSkus = (q: string, page: number) =>
    useQuery({
        queryKey: [...salesSelectionKeys.all, "source-skus", q, page],
        queryFn: () => fetchSourceSkus(q, page),
    })
