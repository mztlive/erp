"use client"

import { useQuery } from "@tanstack/react-query"
import {
    fetchInquiryCandidates,
    type InquiryCandidateQuery,
} from "../api/inquiry-candidates"

export function useInquiryCandidates(query: InquiryCandidateQuery) {
    return useQuery({
        queryKey: ["sales-inquiries", "sales-order-candidates", query],
        queryFn: () => fetchInquiryCandidates(query),
        staleTime: 0,
        retry: false,
    })
}
