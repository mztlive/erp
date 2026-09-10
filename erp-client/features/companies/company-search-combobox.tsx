"use client"
import { useEffect, useState } from "react"
import { OptionCombobox } from "@/components/business"
import { useCompaniesQuery, useCompanyQuery } from "./queries"

/** 只选择我方启用公司，停用的历史已选值仍可回显。 */
export const CompanySearchCombobox = ({
    id,
    value,
    onValueChange,
    disabled,
    placeholder,
}: {
    id: string
    value?: string
    onValueChange: (value?: string) => void
    disabled?: boolean
    placeholder?: string
}) => {
    const [search, setSearch] = useState("")
    const [keyword, setKeyword] = useState("")
    useEffect(() => {
        const timer = setTimeout(() => setKeyword(search.trim()), 250)
        return () => clearTimeout(timer)
    }, [search])
    const list = useCompaniesQuery({
        keyword,
        status: "active",
        page_size: 100,
    })
    const selected = useCompanyQuery(value)
    const rows = [...(list.data?.items ?? [])]
    if (selected.data && !rows.some((row) => row.id === selected.data?.id))
        rows.unshift(selected.data)
    return (
        <OptionCombobox
            id={id}
            value={value ?? null}
            onValueChange={(next) => onValueChange(next ?? undefined)}
            options={rows.map((row) => ({
                value: row.id,
                label:
                    row.legal_name +
                    (row.status === "disabled" ? "（已停用）" : ""),
                disabled: row.status === "disabled",
            }))}
            disabled={disabled}
            loading={list.isFetching || selected.isFetching}
            filterMode="remote"
            onSearchChange={setSearch}
            placeholder={placeholder ?? "请选择公司主体"}
            allowClear
            className="w-full"
            emptyLabel={
                list.isError
                    ? "公司主体加载失败，请重新搜索"
                    : "没有匹配的公司，请先到公司主体维护中登记"
            }
        />
    )
}
